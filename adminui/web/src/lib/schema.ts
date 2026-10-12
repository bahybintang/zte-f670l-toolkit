// Mirrors adminui/schema/SCHEMA.md. The server sends the schema at /api/schema.

export type FieldType =
  | 'string' | 'int' | 'bool' | 'enum' | 'ipv4' | 'ipv6' | 'mac' | 'password' | 'text' | 'hex'

export interface Option { value: string | number; label: string }

export interface Field {
  dm: string
  label?: string
  type?: FieldType
  min?: number
  max?: number
  pattern?: string
  on?: string
  off?: string
  options?: Option[]
  unit?: string
  placeholder?: string
  help?: string
  readonly?: boolean
  advanced?: boolean
  column?: boolean
  required?: boolean
  showIf?: { dm: string; in: string[] }
  default?: string
  liveKey?: string
}

export interface Section {
  id: string
  title?: string
  table?: string
  mode?: 'single' | 'list' | 'readonly'
  row?: number | string
  rowMap?: Record<string, number>
  rowFilter?: { dm: string; in?: string[]; eq?: string }
  rowLabel?: string
  addable?: boolean
  deletable?: boolean
  maxRows?: number
  source?: string
  fields: Field[]
  description?: string
}

export interface Action { id: string; label: string; danger?: string; kind?: string; note?: string }

export interface Page {
  id: string
  title: string
  group: string
  order?: number
  icon?: string
  description?: string
  applicable?: boolean | 'unknown'
  applicableNote?: string
  apply?: 'live' | 'save' | 'reboot' | 'unknown'
  danger?: 'none' | 'confirm' | 'commit-confirm'
  hook?: string
  selector?: { label: string; options: Option[] }
  sections: Section[]
  actions?: Action[]
  notes?: string
  stock?: { menu?: string | null; page?: string }
}

export interface Schema { pages: Page[] }

export interface RowData { no: number; values: Record<string, string>; secretSet: Record<string, boolean> }
export interface SectionData { rows?: RowData[]; hidden?: boolean; error?: string; [k: string]: unknown }

export const GROUPS: { id: string; label: string }[] = [
  { id: 'status', label: 'Status' },
  { id: 'network', label: 'Network' },
  { id: 'wlan', label: 'Wi-Fi' },
  { id: 'security', label: 'Security' },
  { id: 'application', label: 'Applications' },
  { id: 'admin', label: 'Administration' },
  { id: 'diagnostics', label: 'Diagnostics' },
]

export function fieldType(f: Field): FieldType {
  return f.type ?? 'string'
}

export function boolValues(f: Field) {
  return { on: f.on ?? '1', off: f.off ?? '0' }
}

export function optValue(o: Option) {
  return String(o.value)
}

/** Client-side validation; the server repeats it authoritatively. */
export function validate(f: Field, v: string): string | null {
  const t = fieldType(f)
  const name = f.label ?? f.dm
  if (f.required && v === '') return `${name} is required`
  if (v === '' && t !== 'string' && t !== 'password') return null
  switch (t) {
    case 'int': {
      if (!/^-?\d+$/.test(v)) return `${name} must be a whole number`
      const n = Number(v)
      if (f.min !== undefined && n < f.min) return `${name} must be at least ${f.min}`
      if (f.max !== undefined && n > f.max) return `${name} must be at most ${f.max}`
      break
    }
    case 'ipv4':
      if (!/^((25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(25[0-5]|2[0-4]\d|1?\d?\d)$/.test(v)) return `${name} must be an IPv4 address`
      break
    case 'mac':
      if (!/^[0-9a-fA-F]{2}([:-][0-9a-fA-F]{2}){5}$/.test(v)) return `${name} must be a MAC address like 00:11:22:33:44:55`
      break
    case 'ipv6':
      if (!/^[0-9a-fA-F:.]+$/.test(v) || !v.includes(':')) return `${name} must be an IPv6 address`
      break
    case 'enum':
      if (f.options?.length && !f.options.some((o) => optValue(o) === v)) return `Choose a value for ${name}`
      break
    case 'bool':
      break
    default: {
      const len = [...v].length
      if (t === 'password' && v === '') return null
      if (f.min !== undefined && len < f.min) return `${name} needs at least ${f.min} characters`
      if (f.max !== undefined && len > f.max) return `${name} allows at most ${f.max} characters`
    }
  }
  if (f.pattern && v !== '') {
    try {
      if (!new RegExp(f.pattern).test(v)) return `${name} has characters that are not allowed`
    } catch { /* bad pattern in schema: server decides */ }
  }
  return null
}

export function isVisible(f: Field, values: Record<string, string>) {
  if (!f.showIf) return true
  return f.showIf.in.includes(values[f.showIf.dm] ?? '')
}

export function sectionWritable(s: Section) {
  return (s.source ?? 'db') === 'db' && !!s.table && s.mode !== 'readonly'
}
