import { useState } from 'react'
import { Eye, EyeOff } from 'lucide-react'
import { cn } from '@/lib/utils'
import { boolValues, fieldType, optValue, type Field } from '@/lib/schema'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

interface Props {
  field: Field
  id: string
  value: string
  dirty: boolean
  error?: string | null
  secretSet?: boolean
  disabled?: boolean
  onChange: (v: string) => void
}

export function FieldControl({ field, id, value, dirty, error, secretSet, disabled, onChange }: Props) {
  const t = fieldType(field)
  const ro = field.readonly || disabled
  const ring = cn(dirty && 'border-fiber ring-2 ring-fiber/35', error && 'border-destructive ring-2 ring-destructive/20')

  if (t === 'bool') {
    const { on, off } = boolValues(field)
    return (
      <Switch
        id={id}
        checked={value === on}
        disabled={ro}
        onCheckedChange={(c) => onChange(c ? on : off)}
        className={cn(dirty && 'ring-2 ring-fiber/50 ring-offset-2 ring-offset-card')}
      />
    )
  }

  if (t === 'enum' && field.options?.length) {
    const items = field.options.map((o) => ({ value: optValue(o), label: o.label || optValue(o) }))
    const known = items.some((i) => i.value === value)
    if (!known && value !== '') items.push({ value, label: `${value} (current)` })
    return (
      <Select items={items} value={value} disabled={ro} onValueChange={(v) => onChange(String(v ?? ''))}>
        <SelectTrigger id={id} className={cn('h-9 w-full max-w-sm bg-card', ring)} aria-invalid={!!error}>
          <SelectValue placeholder="Choose…" />
        </SelectTrigger>
        <SelectContent>
          {items.map((i) => (
            <SelectItem key={i.value} value={i.value}>
              {i.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    )
  }

  if (t === 'password') return <PasswordInput {...{ field, id, value, ro, ring, error, secretSet, onChange }} />

  if (t === 'text') {
    return (
      <Textarea
        id={id}
        value={value}
        readOnly={ro}
        onChange={(e) => onChange(e.target.value)}
        className={cn('max-w-xl bg-card', ring)}
        aria-invalid={!!error}
      />
    )
  }

  const numeric = t === 'int'
  const narrow = numeric || t === 'ipv4' || t === 'mac'
  return (
    <div className="flex items-center gap-2">
      <Input
        id={id}
        value={value}
        readOnly={ro}
        inputMode={numeric ? 'numeric' : undefined}
        spellCheck={false}
        placeholder={field.placeholder}
        maxLength={t === 'string' && field.max ? field.max : undefined}
        onChange={(e) => onChange(e.target.value)}
        className={cn('h-9 bg-card', narrow ? 'w-44' : 'w-full max-w-sm', (narrow || t === 'hex' || t === 'ipv6') && 'num', ring, ro && 'bg-muted')}
        aria-invalid={!!error}
      />
      {field.unit && <span className="text-sm text-muted-foreground">{field.unit}</span>}
    </div>
  )
}

function PasswordInput({
  field, id, value, ro, ring, error, secretSet, onChange,
}: { field: Field; id: string; value: string; ro?: boolean; ring: string; error?: string | null; secretSet?: boolean; onChange: (v: string) => void }) {
  const [show, setShow] = useState(false)
  return (
    <div className="relative w-full max-w-sm">
      <Input
        id={id}
        type={show ? 'text' : 'password'}
        value={value}
        readOnly={ro}
        autoComplete="new-password"
        placeholder={secretSet ? 'Unchanged — type to replace' : 'Not set'}
        onChange={(e) => onChange(e.target.value)}
        className={cn('h-9 bg-card pr-10', ring)}
        aria-invalid={!!error}
        aria-describedby={`${id}-hint`}
      />
      <button
        type="button"
        onClick={() => setShow((s) => !s)}
        className="absolute inset-y-0 right-0 grid w-9 place-items-center text-muted-foreground hover:text-foreground"
        aria-label={show ? 'Hide password' : 'Show password'}
      >
        {show ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
      </button>
      <span id={`${id}-hint`} className="sr-only">
        {field.min ? `At least ${field.min} characters.` : ''} The current password is never shown.
      </span>
    </div>
  )
}

/** Read-only rendering of a stored value, with enum/bool labels. */
export function displayValue(f: Field, v: string | undefined): string {
  if (v === undefined || v === '') return '–'
  const t = fieldType(f)
  if (t === 'bool') return v === boolValues(f).on ? 'Yes' : 'No'
  if (t === 'enum') return f.options?.find((o) => optValue(o) === v)?.label ?? v
  if (t === 'password') return '••••••'
  return f.unit ? `${v} ${f.unit}` : v
}
