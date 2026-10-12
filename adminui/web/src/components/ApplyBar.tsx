import { useState } from 'react'
import { displayValue } from './FieldControl'
import type { Field, Section } from '@/lib/schema'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'

export type Staged =
  | { kind: 'change'; section?: Section; field?: Field; row?: number; old: string; value: string }
  | { kind: 'add'; section?: Section; values: Record<string, string> }
  | { kind: 'delete'; section?: Section; row: number }

/** The fiber-yellow strip: something is staged and not yet on the router. */
export function ApplyBar({
  count, staged, busy, hasErrors, danger, onDiscard, onApply,
}: {
  count: number
  staged: Staged[]
  busy: boolean
  hasErrors: boolean
  danger?: string
  onDiscard: () => void
  onApply: () => void
}) {
  const [review, setReview] = useState(false)
  const [typed, setTyped] = useState('')
  if (!count) return null
  const needsTyped = danger === 'confirm'
  return (
    <>
      <div className="fixed inset-x-0 bottom-0 z-30 lg:left-[264px]" role="region" aria-label="Pending changes">
        <div className="mx-auto mb-4 flex w-[calc(100%-2rem)] max-w-[1056px] flex-wrap items-center gap-3 rounded-xl border border-fiber bg-card py-2.5 pr-2.5 pl-4 shadow-[0_18px_40px_-18px_rgb(22_32_46/0.45)]">
          <span className="h-6 w-1.5 rounded-full bg-fiber" aria-hidden />
          <span className="flex-1 text-[15px]">
            <b className="font-semibold">{count}</b> {count === 1 ? 'change' : 'changes'} not applied yet
            {hasErrors && <span className="ml-2 text-destructive">· fix the highlighted fields</span>}
          </span>
          <Button variant="ghost" onClick={onDiscard} disabled={busy}>Discard</Button>
          <Button variant="outline" onClick={() => setReview(true)} disabled={busy}>Review</Button>
          <Button
            onClick={() => (needsTyped || danger === 'commit-confirm' ? setReview(true) : onApply())}
            disabled={busy || hasErrors}
            className="bg-fiber text-fiber-ink hover:bg-fiber/85"
          >
            {busy ? 'Applying…' : 'Apply'}
          </Button>
        </div>
      </div>
      <Dialog open={review} onOpenChange={setReview}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Review changes</DialogTitle>
            <DialogDescription>
              {danger === 'commit-confirm'
                ? 'These settings can cut your connection to the router. After applying you have 3 minutes to keep them; otherwise they revert on their own.'
                : 'A backup of the configuration is taken before anything is written.'}
            </DialogDescription>
          </DialogHeader>
          <ul className="max-h-[50dvh] divide-y divide-border overflow-y-auto rounded-lg border border-border">
            {staged.map((s, i) => (
              <li key={i} className="px-3 py-2 text-[14px]">
                {s.kind === 'change' && s.field && (
                  <div className="flex flex-wrap items-baseline justify-between gap-x-4">
                    <span className="font-medium">{s.field.label ?? s.field.dm}</span>
                    <span className="num text-muted-foreground">
                      {s.field.type === 'password' ? 'new password' : (
                        <>
                          <s>{displayValue(s.field, s.old)}</s> <span className="text-foreground">{displayValue(s.field, s.value)}</span>
                        </>
                      )}
                    </span>
                  </div>
                )}
                {s.kind === 'add' && <span>Add a {s.section?.title?.toLowerCase() ?? 'row'} entry</span>}
                {s.kind === 'delete' && <span>Delete {s.section?.title?.toLowerCase() ?? 'row'} #{s.row + 1}</span>}
              </li>
            ))}
          </ul>
          {needsTyped && (
            <label className="block text-sm">
              Type <b>APPLY</b> to confirm
              <input
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                className="mt-1 h-9 w-full rounded-md border border-input bg-card px-2.5"
                autoFocus
              />
            </label>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={() => setReview(false)}>Back</Button>
            <Button
              className="bg-fiber text-fiber-ink hover:bg-fiber/85"
              disabled={busy || hasErrors || (needsTyped && typed !== 'APPLY')}
              onClick={() => {
                setReview(false)
                setTyped('')
                onApply()
              }}
            >
              Apply {count} {count === 1 ? 'change' : 'changes'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
