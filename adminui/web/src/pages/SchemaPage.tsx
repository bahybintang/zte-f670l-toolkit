import { useEffect, useMemo, useState } from 'react'
import { useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Info } from 'lucide-react'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { usePageData, useSchema, useInvalidatePages } from '@/lib/queries'
import { optValue, sectionWritable, validate, type Page, type Section } from '@/lib/schema'
import { pending } from '@/lib/pending'
import { SectionView } from '@/components/SectionView'
import { ApplyBar, type Staged } from '@/components/ApplyBar'
import { PageActions } from '@/components/PageActions'
import { DiagnosticRunner } from '@/components/DiagnosticRunner'

// Diagnostic pages that run a live tool rather than show request/result views.
const DIAG_TOOLS: Record<string, { tool: string; label: string }> = {
  'diag-ping': { tool: 'ping', label: 'Ping' },
  'diag-traceroute': { tool: 'traceroute', label: 'Traceroute' },
}
import { Skeleton } from '@/components/ui/skeleton'
import { NotFound } from './NotFound'
import { Mesh } from './Mesh'

// Schema pages served by a dedicated component instead of generic sections.
const CUSTOM: Record<string, () => React.JSX.Element> = { 'wlan-mesh': Mesh }

export interface Edits {
  /** key "section|row|dm" -> new value */
  changes: Record<string, string>
  adds: { section: string; values: Record<string, string> }[]
  deletes: { section: string; row: number }[]
}

const EMPTY: Edits = { changes: {}, adds: [], deletes: [] }

export function key(section: string, row: number, dm: string) {
  return `${section}|${row}|${dm}`
}

export function SchemaPage() {
  const { pageId } = useParams({ from: '/p/$pageId' })
  const schema = useSchema()
  const page = schema.data?.pages?.find((p) => p.id === pageId)
  if (schema.isPending) return <PageSkeleton />
  if (!page) return <NotFound />
  const Custom = CUSTOM[page.id]
  if (Custom) return <Custom key={page.id} />
  return <PageBody key={page.id} page={page} />
}

function PageBody({ page }: { page: Page }) {
  const search = useSearch({ strict: false }) as { sel?: string }
  const navigate = useNavigate()
  const sel = page.selector ? (search.sel !== undefined ? String(search.sel) : optValue(page.selector.options[0])) : undefined
  const data = usePageData(page.id, sel)
  const invalidate = useInvalidatePages()
  const qc = useQueryClient()
  const [edits, setEdits] = useState<Edits>(EMPTY)
  const [busy, setBusy] = useState(false)
  const [showAdvanced, setShowAdvanced] = useState(false)

  useEffect(() => setEdits(EMPTY), [sel])

  const dirtyCount = Object.keys(edits.changes).length + edits.adds.length + edits.deletes.length

  // Warn before leaving with staged changes.
  useEffect(() => {
    if (!dirtyCount) return
    const h = (e: BeforeUnloadEvent) => e.preventDefault()
    window.addEventListener('beforeunload', h)
    return () => window.removeEventListener('beforeunload', h)
  }, [dirtyCount])

  const sections = data.data?.sections ?? {}

  const staged: Staged[] = useMemo(() => {
    const out: Staged[] = []
    for (const [k, v] of Object.entries(edits.changes)) {
      const [sid, row, dm] = k.split('|')
      const sec = page.sections.find((s) => s.id === sid)
      const f = sec?.fields.find((x) => x.dm === dm)
      const old = sections[sid]?.rows?.find((r) => r.no === Number(row))?.values[dm] ?? ''
      out.push({ kind: 'change', section: sec, field: f, row: Number(row), old, value: v })
    }
    for (const a of edits.adds) out.push({ kind: 'add', section: page.sections.find((s) => s.id === a.section), values: a.values })
    for (const d of edits.deletes) out.push({ kind: 'delete', section: page.sections.find((s) => s.id === d.section), row: d.row })
    return out
  }, [edits, page, sections])

  const errors = useMemo(() => {
    const e: Record<string, string> = {}
    for (const s of staged) {
      if (s.kind === 'change' && s.field) {
        const msg = validate(s.field, s.value)
        if (msg) e[key(s.section!.id, s.row!, s.field.dm)] = msg
      }
    }
    return e
  }, [staged])

  const send = async (body: object, title = page.title) => {
    const r = await api.post<{ entry: number; apply?: string; warnings: string[]; confirm?: { token: string; deadlineSecs: number } }>(
      `/page/${page.id}/apply`,
      { sel, ...body },
    )
    if (r.confirm) {
      pending.add({ token: r.confirm.token, entry: r.entry, title, deadline: Date.now() + r.confirm.deadlineSecs * 1000 })
    }
    r.warnings?.forEach((w) => toast.warning(w))
    return r
  }

  const apply = async (acknowledge: string[] = []) => {
    setBusy(true)
    try {
      const sendResult = send({
        acknowledge,
        changes: Object.entries(edits.changes).map(([k, value]) => {
          const [section, row, dm] = k.split('|')
          const sec = page.sections.find((s) => s.id === section)
          return { section, dm, value, row: sec?.mode === 'list' ? Number(row) : undefined }
        }),
        adds: edits.adds,
        deletes: edits.deletes,
      })
      const r = await sendResult
      toast.success(APPLIED[r.apply ?? 'unknown'])
      setEdits(EMPTY)
      await invalidate()
      qc.invalidateQueries({ queryKey: ['session'] })
    } catch (e) {
      const msg = (e as Error).message
      const m = /^LOCKOUT:([\w-]+): (.*)$/.exec(msg)
      if (m && window.confirm(m[2])) {
        setBusy(false)
        return apply([...acknowledge, m[1]])
      }
      toast.error(m ? 'Not applied.' : msg)
    } finally {
      setBusy(false)
    }
  }

  /** One switch on a harmless page: apply at once instead of staging. */
  const instant = async (section: Section, row: number, dm: string, value: string) => {
    try {
      await send({ changes: [{ section: section.id, dm, value, row: section.mode === 'list' ? row : undefined }] })
      toast.success('Applied.')
      await invalidate()
      qc.invalidateQueries({ queryKey: ['session'] })
    } catch (e) {
      toast.error((e as Error).message)
      await invalidate()
    }
  }
  const allowInstant = (page.danger ?? 'none') === 'none' && dirtyCount === 0

  const hasAdvanced = page.sections.some((s) => s.fields.some((f) => f.advanced))

  const diag = DIAG_TOOLS[page.id]
  if (diag) {
    return (
      <article>
        <header className="mb-7 max-w-3xl">
          <h1 className="font-heading text-[30px] leading-tight font-semibold tracking-tight sm:text-[34px]">{page.title}</h1>
          {page.description && <p className="mt-1.5 text-[15px] text-muted-foreground">{page.description}</p>}
        </header>
        <DiagnosticRunner tool={diag.tool} label={diag.label} />
      </article>
    )
  }

  return (
    <article>
      <header className="mb-7 max-w-3xl">
        <h1 className="font-heading text-[30px] leading-tight font-semibold tracking-tight sm:text-[34px]">{page.title}</h1>
        {page.description && <p className="mt-1.5 text-[15px] text-muted-foreground">{page.description}</p>}
        <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-[13px] text-muted-foreground">
          {page.sections.some(sectionWritable) && <ApplyMode mode={page.apply} />}
          {page.danger === 'commit-confirm' && (
            <span className="flex items-center gap-1.5">
              <AlertTriangle className="size-3.5 text-led-warn" /> Changes revert after 3 minutes unless you keep them
            </span>
          )}
        </div>
      </header>

      {page.applicable !== true && (
        <Notice tone={page.applicable === false ? 'muted' : 'warn'}>
          {page.applicable === false ? 'This page does not apply to this router.' : 'This feature may not be used on this router.'}{' '}
          {page.applicableNote}
        </Notice>
      )}

      {page.selector && (
        <div className="mb-7" role="tablist" aria-label={page.selector.label}>
          <div className="mb-2 text-[13px] font-medium text-muted-foreground">{page.selector.label}</div>
          <div className="flex flex-wrap gap-1.5">
            {page.selector.options.map((o) => {
              const v = optValue(o)
              const active = v === sel
              return (
                <button
                  key={v}
                  role="tab"
                  aria-selected={active}
                  onClick={() => {
                    if (dirtyCount && !confirm('Discard the changes you have not applied?')) return
                    navigate({ to: '.', search: { sel: v } as never, replace: true })
                  }}
                  className={
                    active
                      ? 'rounded-md border border-foreground bg-foreground px-3 py-1.5 text-[14px] font-medium text-background'
                      : 'rounded-md border border-input bg-card px-3 py-1.5 text-[14px] hover:border-foreground/40'
                  }
                >
                  {o.label}
                </button>
              )
            })}
          </div>
        </div>
      )}

      {hasAdvanced && (
        <label className="mb-5 flex w-fit cursor-pointer items-center gap-2 text-sm text-muted-foreground">
          <input type="checkbox" checked={showAdvanced} onChange={(e) => setShowAdvanced(e.target.checked)} className="accent-foreground" />
          Show advanced settings
        </label>
      )}

      {data.isPending ? (
        <PageSkeleton />
      ) : data.isError ? (
        <Notice tone="err">{(data.error as Error).message}</Notice>
      ) : (
        <div className="space-y-11">
          {page.sections.map((s) => (
            <SectionView
              key={s.id}
              section={s}
              data={sections[s.id]}
              edits={edits}
              errors={errors}
              showAdvanced={showAdvanced}
              setEdits={setEdits}
              onInstant={allowInstant ? (row, dm, v) => instant(s, row, dm, v) : undefined}
            />
          ))}
        </div>
      )}

      {page.actions?.length ? <PageActions page={page} /> : null}

      <ApplyBar
        count={dirtyCount}
        staged={staged}
        busy={busy}
        hasErrors={Object.keys(errors).length > 0}
        danger={page.danger}
        onDiscard={() => setEdits(EMPTY)}
        onApply={() => apply()}
      />
    </article>
  )
}

const APPLIED: Record<string, string> = {
  live: 'Applied. The change is active now.',
  save: 'Saved. The router applies it right away.',
  reboot: 'Saved. It takes effect when the router restarts.',
  unknown: 'Saved.',
}

function ApplyMode({ mode }: { mode?: string }) {
  const text: Record<string, string> = {
    live: 'Changes apply instantly',
    save: 'Changes are saved and applied by the router',
    reboot: 'Changes take effect after a restart',
    unknown: 'Changes are saved; some may need a restart',
  }
  const led = mode === 'live' ? 'ok' : mode === 'reboot' ? 'warn' : undefined
  return (
    <span className="flex items-center gap-1.5">
      <span className="led" data-state={led} /> {text[mode ?? 'unknown']}
    </span>
  )
}

export function Notice({ tone, children }: { tone: 'warn' | 'err' | 'muted'; children: React.ReactNode }) {
  const cls = {
    warn: 'border-led-warn/45 bg-led-warn/10',
    err: 'border-destructive/45 bg-destructive/8 text-destructive',
    muted: 'border-border bg-muted/60 text-muted-foreground',
  }[tone]
  return (
    <div className={`mb-6 flex max-w-3xl gap-2.5 rounded-lg border px-3.5 py-2.5 text-sm ${cls}`}>
      <Info className="mt-0.5 size-4 flex-none" />
      <div>{children}</div>
    </div>
  )
}

function PageSkeleton() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-8 w-64" />
      <Skeleton className="h-4 w-96 max-w-full" />
      <div className="space-y-3 pt-6">
        {[0, 1, 2, 3].map((i) => (
          <Skeleton key={i} className="h-9 w-full max-w-2xl" />
        ))}
      </div>
    </div>
  )
}
