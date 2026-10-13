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
export interface SectionData {
  rows?: RowData[]
  hidden?: boolean
  error?: string
  /** Live source has no data path on this firmware — render a calm "not available" state. */
  unavailable?: boolean
  unavailableNote?: string
  [k: string]: unknown
}

export const GROUPS: { id: string; label: string }[] = [
  { id: 'status', label: 'Status' },
  { id: 'network', label: 'Network' },
  { id: 'wlan', label: 'Wi-Fi' },
  { id: 'security', label: 'Security' },
  { id: 'application', label: 'Applications' },
  { id: 'admin', label: 'Administration' },
  { id: 'diagnostics', label: 'Diagnostics' },
]

/**
 * Sub-sections within a group, in display order, each an explicit list of page
 * ids. A page not listed here falls into the group's "More" bucket, so the nav
 * degrades gracefully if the page map ever drifts from the schema.
 */
export const SUBGROUPS: Record<string, { label: string; ids: string[] }[]> = {
  network: [
    { label: 'Internet', ids: ['wan', 'wan-user-pppoe', 'dslite', 'mtu', 'dhcp-client', 'wan-client-limit', 'arp-detect', 'vlan-binding'] },
    { label: 'LAN & DHCP', ids: ['lan', 'dhcp-server', 'dhcp-static', 'dhcp-device-pools', 'dhcp-port'] },
    { label: 'IPv6', ids: ['ipv6-ra', 'dhcpv6-server', 'ipv6-lan-prefix', 'prefix'] },
    { label: 'Routing', ids: ['static-routes', 'route-table', 'rip'] },
    { label: 'Traffic & QoS', ids: ['qos', 'qos-queues', 'qos-rules', 'qos-app', 'qos-policers', 'qos-template'] },
    { label: 'Remote management', ids: ['tr069', 'tr069-cert'] },
    { label: 'Time', ids: ['sntp'] },
  ],
  wlan: [
    { label: 'Radio', ids: ['wlan-radio', 'wlan-radio-user', 'wlan-schedule', 'wlan-wmm', 'wlan-mesh', 'wlan-steering'] },
    { label: 'Networks', ids: ['wlan-ssids', 'wlan-security', 'wlan-wps', 'wlan-mac-acl'] },
  ],
  security: [
    { label: 'Firewall', ids: ['sec-firewall-level', 'sec-firewall-attack', 'sec-service-control', 'sec-service-ports'] },
    { label: 'Filters', ids: ['sec-url-filter', 'sec-mac-filter', 'sec-ip-filter'] },
    { label: 'Access', ids: ['sec-telnet'] },
  ],
  application: [
    { label: 'Sharing', ids: ['usb-ftp-download', 'usb-iptv', 'samba', 'dms'] },
    { label: 'Port forwarding & NAT', ids: ['app-port-forwarding', 'app-dmz', 'app-alg', 'app-upnp'] },
    { label: 'DNS', ids: ['dns-domain', 'dns-hosts', 'app-ddns'] },
    { label: 'Multicast', ids: ['igmp-snooping', 'igmp-proxy', 'mld-snooping', 'mld-proxy'] },
    { label: 'Voice (VoIP)', ids: ['voip-sip', 'voip-accounts', 'voip-register', 'voip-services', 'voip-media', 'voip-timers', 'voip-digitmap', 'voip-callerid', 'voip-line-interface', 'voip-qos', 'voip-advanced'] },
  ],
  admin: [
    { label: 'System', ids: ['admin-system', 'admin-users', 'admin-business', 'admin-led', 'help'] },
    { label: 'Backup & firmware', ids: ['admin-upgrade', 'admin-usb-backup', 'admin-usb-restore', 'admin-maintenance-report'] },
    { label: 'Logs & monitoring', ids: ['admin-log', 'admin-monitor', 'admin-loopback'] },
    { label: 'Access', ids: ['admin-telnet', 'admin-voip-protocol'] },
  ],
  status: [
    { label: 'Overview', ids: ['status-device', 'status-service-config', 'status-alarms'] },
    { label: 'Internet', ids: ['status-wan-ipv4', 'status-wan-ipv6', 'status-dslite-eth', 'status-pon', 'status-pon-counters'] },
    { label: 'LAN & Wi-Fi', ids: ['status-lan-clients', 'status-wlan', 'status-ethernet', 'status-usb'] },
    { label: 'Voice', ids: ['status-voip', 'status-voip-accounts'] },
    { label: 'Remote', ids: ['status-tr069'] },
  ],
  diagnostics: [
    { label: 'Network tools', ids: ['diag-ping', 'diag-traceroute', 'diag-arp', 'diag-mac-table'] },
    { label: 'Line & mirror', ids: ['diag-mirror', 'diag-monitor'] },
    { label: 'Voice & remote', ids: ['diag-voice', 'diag-simulation', 'diag-tr069-inform'] },
  ],
}

/** The ordered sub-sections for a group's pages; a single unnamed section when the group has no taxonomy. */
export function groupSections(groupId: string, pages: Page[]): { label: string | null; pages: Page[] }[] {
  const subs = SUBGROUPS[groupId]
  if (!subs) return [{ label: null, pages }]
  const byId = new Map(pages.map((p) => [p.id, p]))
  const used = new Set<string>()
  const out: { label: string | null; pages: Page[] }[] = []
  for (const s of subs) {
    const list = s.ids.map((id) => byId.get(id)).filter((p): p is Page => !!p)
    list.forEach((p) => used.add(p.id))
    if (list.length) out.push({ label: s.label, pages: list })
  }
  const rest = pages.filter((p) => !used.has(p.id))
  if (rest.length) out.push({ label: 'More', pages: rest })
  return out
}

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
      if (f.min != null && n < f.min) return `${name} must be at least ${f.min}`
      if (f.max != null && n > f.max) return `${name} must be at most ${f.max}`
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
      if (f.min != null && len < f.min) return `${name} needs at least ${f.min} characters`
      if (f.max != null && len > f.max) return `${name} allows at most ${f.max} characters`
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
