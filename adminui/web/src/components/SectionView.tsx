import { useState, type Dispatch, type SetStateAction } from 'react'
import { Pencil, Plus, Trash2, Undo2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { isVisible, sectionWritable, type Field, type RowData, type Section, type SectionData } from '@/lib/schema'
import { key, type Edits } from '@/pages/SchemaPage'
import { FieldControl, displayValue } from './FieldControl'
import { RowDialog } from './RowDialog'
import { NotAvailable } from './NotAvailable'
import { Button } from '@/components/ui/button'

interface Props {
  section: Section
  data?: SectionData
  edits: Edits
  errors: Record<string, string>
  showAdvanced: boolean
  setEdits: Dispatch<SetStateAction<Edits>>
  onInstant?: (row: number, dm: string, value: string) => void
}

export function SectionView(p: Props) {
  const { section, data } = p
  if (data?.hidden) return null
  const isRuntime = (section.source ?? 'db').startsWith('runtime:')
  return (
    <section aria-labelledby={`s-${section.id}`}>
      <div className="mb-3 flex items-end justify-between gap-4 border-b border-border pb-2">
        <div>
          <h2 id={`s-${section.id}`} className="font-heading text-[19px] font-semibold">
            {section.title ?? section.id}
          </h2>
          {section.description && <p className="text-sm text-muted-foreground">{section.description}</p>}
        </div>
        {isRuntime && <span className="flex items-center gap-1.5 text-xs text-muted-foreground"><span className="led" data-state="ok" />Live</span>}
      </div>
      {data?.unavailable ? (
        <NotAvailable note={data.unavailableNote} />
      ) : data?.error ? (
        <p className="text-sm text-muted-foreground">{data.error}</p>
      ) : isRuntime ? (
        <RuntimeView section={section} data={data} />
      ) : section.mode === 'list' || (section.mode === 'readonly' && (data?.rows?.length ?? 0) > 1) ? (
        <ListView {...p} />
      ) : (
        <SingleView {...p} />
      )}
    </section>
  )
}

function useFields(section: Section, showAdvanced: boolean) {
  return section.fields.filter((f) => showAdvanced || !f.advanced)
}

function SingleView({ section, data, edits, errors, showAdvanced, setEdits, onInstant }: Props) {
  const row = data?.rows?.[0]
  const fields = useFields(section, showAdvanced)
  if (!row) return <p className="text-sm text-muted-foreground">No data.</p>
  const writable = sectionWritable(section)
  const values: Record<string, string> = { ...row.values }
  for (const f of section.fields) {
    const k = key(section.id, row.no, f.dm)
    if (k in edits.changes) values[f.dm] = edits.changes[k]
  }
  const set = (f: Field, v: string) => {
    const k = key(section.id, row.no, f.dm)
    if (onInstant && f.type === 'bool') {
      onInstant(row.no, f.dm, v)
      return
    }
    setEdits((e) => {
      const changes = { ...e.changes }
      if (v === (row.values[f.dm] ?? '') && f.type !== 'password') delete changes[k]
      else if (f.type === 'password' && v === '') delete changes[k]
      else changes[k] = v
      return { ...e, changes }
    })
  }
  return (
    <dl className="divide-y divide-border/70">
      {fields.filter((f) => isVisible(f, values)).map((f) => {
        const k = key(section.id, row.no, f.dm)
        const id = `f-${section.id}-${f.dm}`
        const dirty = k in edits.changes
        return (
          <div key={f.dm} className="grid gap-x-8 gap-y-1.5 py-3 sm:grid-cols-[minmax(180px,260px)_1fr] sm:items-center">
            <dt>
              <label htmlFor={id} className="text-[14.5px] font-medium">
                {f.label ?? f.dm}
              </label>
              {f.help && <p className="text-[13px] leading-snug text-muted-foreground">{f.help}</p>}
            </dt>
            <dd>
              {writable && !f.readonly ? (
                <FieldControl
                  field={f}
                  id={id}
                  value={values[f.dm] ?? ''}
                  dirty={dirty}
                  error={errors[k]}
                  secretSet={row.secretSet?.[f.dm]}
                  onChange={(v) => set(f, v)}
                />
              ) : (
                <span className="num text-[15px]">{displayValue(f, values[f.dm])}</span>
              )}
              {errors[k] && <p className="mt-1 text-[13px] text-destructive">{errors[k]}</p>}
            </dd>
          </div>
        )
      })}
    </dl>
  )
}

function rowTitle(section: Section, r: RowData) {
  if (!section.rowLabel) return `#${r.no + 1}`
  return section.rowLabel.replace(/\{(\w+)\}/g, (_, k) => (k === 'n' ? String(r.no + 1) : (r.values[k] ?? '')))
}

function ListView({ section, data, edits, setEdits }: Props) {
  const [editing, setEditing] = useState<RowData | 'new' | null>(null)
  const writable = sectionWritable(section)
  let rows = data?.rows ?? []
  const rf = section.rowFilter
  if (rf) rows = rows.filter((r) => (rf.in ? rf.in.includes(r.values[rf.dm]) : r.values[rf.dm] === rf.eq))
  const cols = section.fields.filter((f) => f.column).slice(0, 6)
  const shown = cols.length ? cols : section.fields.slice(0, 5)
  const deleted = (no: number) => edits.deletes.some((d) => d.section === section.id && d.row === no)
  const adds = edits.adds.filter((a) => a.section === section.id)
  // maxRows is null (not undefined) when the server omits it — `!= null` catches both,
  // otherwise `>= null` (→ >= 0) is always true and wrongly disables Add on every list.
  const full = section.maxRows != null && rows.length + adds.length >= section.maxRows

  return (
    <div>
      {rows.length === 0 && adds.length === 0 ? (
        <p className="py-3 text-sm text-muted-foreground">Nothing here yet.</p>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-border bg-card">
          <table className="w-full text-[14px]">
            <thead>
              <tr className="border-b border-border text-left text-[13px] text-muted-foreground">
                {shown.map((f) => (
                  <th key={f.dm} className="px-3 py-2 font-medium whitespace-nowrap">{f.label ?? f.dm}</th>
                ))}
                {writable && <th className="w-24" />}
              </tr>
            </thead>
            <tbody className="divide-y divide-border/70">
              {rows.map((r) => {
                const changed = Object.keys(edits.changes).some((k) => k.startsWith(`${section.id}|${r.no}|`))
                const del = deleted(r.no)
                const v = (dm: string) => edits.changes[key(section.id, r.no, dm)] ?? r.values[dm]
                return (
                  <tr key={r.no} className={cn(del && 'opacity-45 line-through', changed && 'bg-fiber/10')}>
                    {shown.map((f) => (
                      <td key={f.dm} className="num px-3 py-2 whitespace-nowrap">{displayValue(f, v(f.dm))}</td>
                    ))}
                    {writable && (
                      <td className="px-2 py-1 text-right whitespace-nowrap">
                        {del ? (
                          <Button variant="ghost" size="icon-sm" aria-label="Keep this row"
                            onClick={() => setEdits((e) => ({ ...e, deletes: e.deletes.filter((d) => !(d.section === section.id && d.row === r.no)) }))}>
                            <Undo2 />
                          </Button>
                        ) : (
                          <>
                            <Button variant="ghost" size="icon-sm" aria-label={`Edit ${rowTitle(section, r)}`} onClick={() => setEditing(r)}>
                              <Pencil />
                            </Button>
                            {section.deletable && (
                              <Button variant="ghost" size="icon-sm" aria-label={`Delete ${rowTitle(section, r)}`}
                                onClick={() => setEdits((e) => ({ ...e, deletes: [...e.deletes, { section: section.id, row: r.no }] }))}>
                                <Trash2 />
                              </Button>
                            )}
                          </>
                        )}
                      </td>
                    )}
                  </tr>
                )
              })}
              {adds.map((a, i) => (
                <tr key={`add-${i}`} className="bg-fiber/12">
                  {shown.map((f) => (
                    <td key={f.dm} className="num px-3 py-2 whitespace-nowrap">{displayValue(f, a.values[f.dm])}</td>
                  ))}
                  <td className="px-2 py-1 text-right">
                    <Button variant="ghost" size="icon-sm" aria-label="Remove new row"
                      onClick={() => setEdits((e) => ({ ...e, adds: e.adds.filter((x) => x !== a) }))}>
                      <Trash2 />
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {writable && section.addable && (
        <Button variant="outline" size="sm" className="mt-3" disabled={full} onClick={() => setEditing('new')}>
          <Plus /> Add {section.title ? section.title.replace(/s$/, '').toLowerCase() : 'row'}
        </Button>
      )}
      {full && <span className="ml-3 text-[13px] text-muted-foreground">Limit of {section.maxRows} reached.</span>}
      {editing && (
        <RowDialog
          section={section}
          row={editing === 'new' ? undefined : editing}
          staged={editing === 'new' ? {} : Object.fromEntries(
            Object.entries(edits.changes)
              .filter(([k]) => k.startsWith(`${section.id}|${editing.no}|`))
              .map(([k, v]) => [k.split('|')[2], v]),
          )}
          onClose={() => setEditing(null)}
          onSave={(vals) => {
            if (editing === 'new') {
              setEdits((e) => ({ ...e, adds: [...e.adds, { section: section.id, values: vals }] }))
            } else {
              setEdits((e) => {
                const changes = { ...e.changes }
                for (const f of section.fields) {
                  const k = key(section.id, editing.no, f.dm)
                  if (!(f.dm in vals)) continue
                  if (vals[f.dm] === (editing.values[f.dm] ?? '')) delete changes[k]
                  else changes[k] = vals[f.dm]
                }
                return { ...e, changes }
              })
            }
            setEditing(null)
          }}
        />
      )}
    </div>
  )
}

/** Live data: show schema fields when present, otherwise whatever the source returns. */
function RuntimeView({ section, data }: { section: Section; data?: SectionData }) {
  if (!data) return <p className="text-sm text-muted-foreground">No data.</p>
  const rows = Array.isArray(data.rows) ? (data.rows as unknown as Record<string, unknown>[]) : null
  const fieldsFor = (obj: Record<string, unknown>) => {
    const known = section.fields.filter((f) => f.dm in obj)
    return known.length ? known : Object.keys(obj).map((k) => ({ dm: k, label: humanize(k) }) as Field)
  }
  const fmt = (f: Field, v: unknown) =>
    v === null || v === undefined || v === '' ? '–' : typeof v === 'boolean' ? (v ? 'Yes' : 'No') : displayValue(f, String(v))
  if (rows) {
    if (!rows.length) return <p className="py-3 text-sm text-muted-foreground">Nothing to show right now.</p>
    const fs = fieldsFor(rows[0]).slice(0, 8)
    return (
      <div className="overflow-x-auto rounded-lg border border-border bg-card">
        <table className="w-full text-[14px]">
          <thead>
            <tr className="border-b border-border text-left text-[13px] text-muted-foreground">
              {fs.map((f) => <th key={f.dm} className="px-3 py-2 font-medium whitespace-nowrap">{f.label ?? f.dm}</th>)}
            </tr>
          </thead>
          <tbody className="divide-y divide-border/70">
            {rows.map((r, i) => (
              <tr key={i}>
                {fs.map((f) => <td key={f.dm} className="num px-3 py-2 whitespace-nowrap">{fmt(f, r[f.dm])}</td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }
  const obj = data as Record<string, unknown>
  return (
    <dl className="grid gap-x-10 gap-y-3 sm:grid-cols-2">
      {fieldsFor(obj).filter((f) => typeof obj[f.dm] !== 'object' || obj[f.dm] === null).map((f) => (
        <div key={f.dm} className="flex items-baseline justify-between gap-4 border-b border-border/60 pb-2">
          <dt className="text-[14px] text-muted-foreground">{f.label ?? f.dm}</dt>
          <dd className="num text-right text-[15px] font-medium">{fmt(f, obj[f.dm])}</dd>
        </div>
      ))}
    </dl>
  )
}

export function humanize(k: string) {
  return k
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/_/g, ' ')
    .replace(/\b(Dbm|Mbps|Ma)\b/gi, (m) => ({ dbm: '(dBm)', mbps: '(Mbit/s)', ma: '(mA)' })[m.toLowerCase()] ?? m)
    .replace(/^./, (c) => c.toUpperCase())
}
