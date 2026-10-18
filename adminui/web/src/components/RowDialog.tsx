import { useState } from 'react'
import { isVisible, validate, type RowData, type Section } from '@/lib/schema'
import { FieldControl } from './FieldControl'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'

export function RowDialog({
  section, row, staged, onClose, onSave,
}: {
  section: Section
  row?: RowData
  staged: Record<string, string>
  onClose: () => void
  onSave: (values: Record<string, string>) => void
}) {
  const editable = section.fields.filter((f) => !f.readonly)
  const [vals, setVals] = useState<Record<string, string>>(() => {
    const v: Record<string, string> = {}
    for (const f of section.fields) v[f.dm] = staged[f.dm] ?? row?.values[f.dm] ?? f.default ?? ''
    return v
  })
  const [initial] = useState(vals)
  const [touched, setTouched] = useState<Set<string>>(() => new Set())
  const [submitted, setSubmitted] = useState(false)
  // Editing a row validates only what the user changed: an unchanged value the
  // router already holds (e.g. no PPPoE username on a bridged row) is not
  // blocked. A new row validates every visible field.
  const errs = Object.fromEntries(
    editable
      .filter((f) => isVisible(f, vals) && (!row || vals[f.dm] !== initial[f.dm]))
      .map((f) => [f.dm, validate(f, vals[f.dm] ?? '')] as const)
      .filter(([, e]) => e),
  )
  // Show a field's error once it has been changed, or after a save attempt.
  const shown = (dm: string) => (submitted || touched.has(dm) ? errs[dm] : undefined)
  const noun = section.title ? section.title.replace(/s$/, '').toLowerCase() : 'row'
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{row ? `Edit ${noun}` : `New ${noun}`}</DialogTitle>
        </DialogHeader>
        <form
          id="rowform"
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault()
            setSubmitted(true)
            if (Object.keys(errs).length) return
            const out: Record<string, string> = {}
            for (const f of editable) if (isVisible(f, vals)) out[f.dm] = vals[f.dm] ?? ''
            onSave(out)
          }}
        >
          {editable.filter((f) => isVisible(f, vals)).map((f) => (
            <div key={f.dm} className="space-y-1.5">
              <label htmlFor={`rd-${f.dm}`} className="text-sm font-medium">{f.label ?? f.dm}</label>
              <FieldControl
                field={f}
                id={`rd-${f.dm}`}
                value={vals[f.dm] ?? ''}
                dirty={false}
                error={shown(f.dm)}
                secretSet={row?.secretSet?.[f.dm]}
                onChange={(v) => {
                  setVals((s) => ({ ...s, [f.dm]: v }))
                  setTouched((t) => (t.has(f.dm) ? t : new Set(t).add(f.dm)))
                }}
              />
              {shown(f.dm) ? (
                <p className="text-[13px] text-destructive">{shown(f.dm)}</p>
              ) : f.help ? (
                <p className="text-[13px] text-muted-foreground">{f.help}</p>
              ) : null}
            </div>
          ))}
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button type="submit" form="rowform" disabled={submitted && Object.keys(errs).length > 0}>
            {row ? 'Stage changes' : `Stage new ${noun}`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
