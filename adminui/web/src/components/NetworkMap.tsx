import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import {
  useHosts,
  useLanPorts,
  useTraffic,
  useWifiClients,
  type Host,
  type LanPort,
  type WifiClient,
} from '@/lib/queries'
import { bitrate } from '@/lib/format'
import { useLeds } from '@/components/LedStrip'
import { RouterGlyph, SignalBars, signalLevel, type MeshNode } from '@/components/MeshMap'
import { Skeleton } from '@/components/ui/skeleton'

/* The home network as one picture: where the internet comes from, this router,
   and everything hanging off it (LAN ports, both radios, mesh devices) with the
   devices that are online on each. Same visual language as the mesh map. */

interface MeshStatus {
  running: boolean
  nodes?: MeshNode[]
  agent?: { controller: string | null } | null
}

interface Device {
  mac: string
  name: string
  ip?: string
  signal?: number
}

interface Group {
  key: string
  kind: 'ethernet' | 'wifi' | 'mesh'
  title: string
  sub: string
  /** Edge from the router: Wi-Fi edges are dashed and flow. */
  wifi: boolean
  /** Live rate through the branch (interface counters, in + out), shown on its card. */
  rate?: number
  devices: Device[]
  ports?: LanPort[]
  empty: string
}

const W = 960
const UP_X = 92
const RT_X = 300
const BR_X = 540
const BR_W = 188
const BR_H = 50
const CHIP_X = 672
const CHIP_W = 138
const CHIP_H = 26
const CHIP_GAP = 8
const CHIP_COLS = 2
const ROW = CHIP_H + CHIP_GAP
const GROUP_GAP = 22
const ROUTER_R = 46

function useMeshStatus() {
  return useQuery({
    queryKey: ['mesh-status'],
    queryFn: () => api.get<{ status: MeshStatus }>('/mesh/status'),
    refetchInterval: 15_000,
  })
}

const isWifiIf = (i?: string) => !!i && i.startsWith('wlan')
/** Reached through this router's Wi-Fi backhaul (the station), i.e. on the main router's side. */
const isUplinkIf = (i?: string) => !!i && i.startsWith('apcli')
/** Entries the host table carries that are not devices: broadcast/multicast and this router itself. */
const notADevice = (h: Host) =>
  (parseInt(h.MAC.slice(0, 2), 16) & 1) === 1 || (!!h.IP && h.IP === window.location.hostname)
const bandOf = (i: string) => (Number(i.slice(4)) >= 4 ? '5' : '2.4')
const rssi = (c?: WifiClient) => {
  const v = Number(c?.RSSI ?? c?.RSSI0)
  return Number.isFinite(v) && v < 0 ? v : undefined
}
const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? '' : 's'}`

/** Online devices, by where they connect, plus mesh devices: one group per branch. */
function buildGroups(
  hosts: Host[],
  wifi: WifiClient[],
  ports: LanPort[],
  mesh: MeshStatus | undefined,
  rates: Record<string, number>,
) {
  const byMac = new Map(wifi.map((c) => [c.mac.toLowerCase(), c]))
  const seen = new Set<string>()
  const toDevice = (h: Host): Device => {
    const c = byMac.get(h.MAC)
    const ip = h.IP && h.IP !== '0.0.0.0' ? h.IP : h.arpIp
    return { mac: h.MAC, name: h.HostName || ip || h.MAC, ip, signal: rssi(c) }
  }
  const band: Record<'2.4' | '5', Device[]> = { '2.4': [], '5': [] }
  const wired: Device[] = []
  const beyond: Device[] = []
  for (const h of hosts) {
    const c = byMac.get(h.MAC)
    if ((h.Active !== '1' && !c) || notADevice(h)) continue
    seen.add(h.MAC)
    const iface = c?.iface ?? h.If
    if (isWifiIf(iface)) band[bandOf(iface!)].push(toDevice(h))
    else if (isUplinkIf(iface)) beyond.push(toDevice(h))
    else wired.push(toDevice(h))
  }
  // Wi-Fi clients the host table has not caught up with yet.
  for (const c of wifi) {
    const mac = c.mac.toLowerCase()
    if (seen.has(mac)) continue
    band[bandOf(c.iface)].push({ mac, name: mac, signal: rssi(c) })
  }
  const byName = (a: Device, b: Device) => a.name.localeCompare(b.name)
  const used = ports.filter((p) => p.link).length
  const groups: Group[] = [
    {
      key: 'eth',
      kind: 'ethernet',
      title: 'Ethernet',
      sub: `${used} of ${ports.length || 4} ports in use`,
      wifi: false,
      devices: wired.sort(byName),
      ports,
      empty: used ? 'No devices seen yet' : 'No cables',
    },
    {
      key: 'w24',
      kind: 'wifi',
      title: 'Wi-Fi 2.4 GHz',
      sub: plural(band['2.4'].length, 'device'),
      wifi: true,
      rate: rates.wlan0,
      devices: band['2.4'].sort(byName),
      empty: 'No devices',
    },
    {
      key: 'w5',
      kind: 'wifi',
      title: 'Wi-Fi 5 GHz',
      sub: plural(band['5'].length, 'device'),
      wifi: true,
      rate: rates.wlan4,
      devices: band['5'].sort(byName),
      empty: 'No devices',
    },
  ]
  if (mesh?.running) {
    for (const n of mesh.nodes ?? []) {
      if (isUplink(n, mesh)) continue // drawn upstream instead
      groups.push({
        key: `mesh-${n.almac}`,
        kind: 'mesh',
        title: n.name ?? n.almac,
        sub: `Mesh · ${plural(n.clients, 'client')}`,
        wifi: !/^LAN|Ethernet/i.test(n.via),
        devices: [],
        empty: `via ${n.via}`,
      })
    }
  }
  return { groups, beyond }
}

/** For a mesh agent, the main router it hangs off (it is upstream, not a branch). */
function isUplink(n: MeshNode, mesh: MeshStatus) {
  return !!mesh.agent && (!!n.link || n.almac === mesh.agent.controller)
}

function groupHeight(g: Group, cap: number) {
  const shown = Math.min(g.devices.length, cap) + (g.devices.length > cap ? 1 : 0)
  const rows = Math.max(1, Math.ceil(shown / CHIP_COLS))
  return Math.max(BR_H, rows * ROW - CHIP_GAP)
}

/** The live network map, with its data. `cap` limits device chips per branch. */
export function NetworkMap({ cap = 6 }: { cap?: number }) {
  const hosts = useHosts()
  const wifi = useWifiClients()
  const ports = useLanPorts()
  const mesh = useMeshStatus()
  const leds = useLeds()
  const traffic = useTraffic()

  const rates = useMemo(() => {
    const r: Record<string, number> = {}
    for (const i of traffic.data?.ifaces ?? []) r[i.name] = (i.rxRate.at(-1) ?? 0) + (i.txRate.at(-1) ?? 0)
    return r
  }, [traffic.data])

  const status = mesh.data?.status
  const { groups, beyond } = useMemo(
    () =>
      buildGroups(
        Array.isArray(hosts.data?.rows) ? hosts.data.rows : [],
        Array.isArray(wifi.data) ? wifi.data : [],
        Array.isArray(ports.data?.rows) ? ports.data.rows : [],
        status,
        rates,
      ),
    [hosts.data, wifi.data, ports.data, status, rates],
  )

  if (hosts.isPending) return <Skeleton className="h-[340px] w-full rounded-xl" />
  if (hosts.isError) return <p className="text-sm text-destructive">{(hosts.error as Error).message}</p>

  const uplink = status?.running ? status.nodes?.find((n) => isUplink(n, status)) : undefined
  const led = (id: string) => leds.data?.find((l) => l.id === id)?.state
  const internet = led('Internet') === 'ok'
  const los = led('LOS') === 'err' || led('LOS') === 'blink'
  return (
    <MapCanvas
      groups={groups}
      cap={cap}
      uplink={uplink}
      beyond={beyond.length}
      internet={internet}
      los={los}
      wanRate={rates.pon0}
    />
  )
}

function MapCanvas({
  groups,
  cap,
  uplink,
  beyond,
  internet,
  los,
  wanRate,
}: {
  groups: Group[]
  cap: number
  uplink?: MeshNode
  beyond: number
  internet: boolean
  los: boolean
  wanRate?: number
}) {
  const heights = groups.map((g) => groupHeight(g, cap))
  const H = Math.max(260, heights.reduce((a, h) => a + h, 0) + GROUP_GAP * (groups.length - 1) + 64)
  let y = (H - (heights.reduce((a, h) => a + h, 0) + GROUP_GAP * (groups.length - 1))) / 2
  const placed = groups.map((g, i) => {
    const top = y
    y += heights[i] + GROUP_GAP
    return { g, top, cy: top + heights[i] / 2 }
  })
  const RY = H / 2
  const online = groups.filter((g) => g.kind !== 'mesh').reduce((a, g) => a + g.devices.length, 0)

  return (
    <figure className="net-map m-0">
      <div className="overflow-x-auto [scrollbar-width:thin]">
        <svg
          viewBox={`0 0 ${W} ${H}`}
          className="h-auto w-full min-w-[720px]"
          role="img"
          aria-label={`Network map: ${uplink ? `connected to ${uplink.name ?? 'the main router'} over ${uplink.via}` : internet ? 'online' : 'internet not connected'}, ${plural(online, 'device')} online`}
        >
          <defs>
            <radialGradient id="net-glow" cx="50%" cy="50%" r="50%">
              <stop offset="0%" className="[stop-color:var(--mesh)]" stopOpacity="0.28" />
              <stop offset="100%" className="[stop-color:var(--mesh)]" stopOpacity="0" />
            </radialGradient>
            <pattern id="net-grid" width="24" height="24" patternUnits="userSpaceOnUse">
              <circle cx="1" cy="1" r="1" className="fill-border" />
            </pattern>
          </defs>
          <rect width={W} height={H} rx="12" fill="url(#net-grid)" />

          <Upstream y={RY} uplink={uplink} beyond={beyond} internet={internet} los={los} rate={wanRate} />

          {/* Router → branches */}
          {placed.map(({ g, cy }) => (
            <Edge key={`e-${g.key}`} x1={RT_X + ROUTER_R} y1={RY} x2={BR_X - BR_W / 2} y2={cy} wifi={g.wifi} />
          ))}

          <circle cx={RT_X} cy={RY} r={ROUTER_R * 2.2} fill="url(#net-glow)" />
          <circle cx={RT_X} cy={RY} r={ROUTER_R} className="fill-card [stroke:var(--mesh)]" strokeWidth={2.5} />
          <RouterGlyph x={RT_X} y={RY - 10} />
          <text x={RT_X} y={RY + 18} textAnchor="middle" className="fill-foreground text-[12px] font-semibold">
            This router
          </text>
          <text x={RT_X} y={RY + ROUTER_R + 18} textAnchor="middle" className="fill-muted-foreground text-[11px]">
            {plural(online, 'device')} online
          </text>

          {placed.map(({ g, cy }) => (
            <Branch key={g.key} g={g} cy={cy} cap={cap} />
          ))}
        </svg>
      </div>
      <style>{`
        .net-map { --mesh: #b8860b; }
        .dark .net-map { --mesh: var(--color-fiber); }
        .net-flow { animation: net-flow 1.2s linear infinite; }
        .net-chip { transition: opacity 160ms ease; }
        .net-chip:hover { opacity: 0.8; }
        @keyframes net-flow { to { stroke-dashoffset: -12; } }
        @media (prefers-reduced-motion: reduce) { .net-flow { animation: none; } }
      `}</style>
      <figcaption className="mt-2 flex flex-wrap gap-x-5 gap-y-1 text-[12px] text-muted-foreground">
        <span className="inline-flex items-center gap-1.5">
          <svg width="22" height="6" aria-hidden>
            <line x1="1" y1="3" x2="21" y2="3" className="[stroke:var(--mesh)]" strokeWidth="3" strokeLinecap="round" />
          </svg>
          Cable
        </span>
        <span className="inline-flex items-center gap-1.5">
          <svg width="22" height="6" aria-hidden>
            <line x1="1" y1="3" x2="21" y2="3" className="[stroke:var(--mesh)]" strokeWidth="2" strokeDasharray="4 4" />
          </svg>
          Wi-Fi
        </span>
        <span className="inline-flex items-center gap-1.5">
          <span className="led" data-state="ok" /> Online
        </span>
        <span className="num">Rates are live (in + out).</span>
      </figcaption>
    </figure>
  )
}

/** A link: a smooth horizontal S-curve, dashed and flowing for Wi-Fi. */
function Edge({
  x1,
  y1,
  x2,
  y2,
  wifi,
  dim,
}: {
  x1: number
  y1: number
  x2: number
  y2: number
  wifi: boolean
  dim?: boolean
}) {
  const mx = (x1 + x2) / 2
  const d = `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`
  return (
    <g>
      <path
        d={d}
        fill="none"
        className={`[stroke:var(--mesh)] ${wifi ? 'net-flow' : ''}`}
        strokeWidth={dim ? 1.2 : wifi ? 2 : 3}
        strokeDasharray={wifi ? '6 6' : undefined}
        strokeLinecap="round"
        opacity={dim ? 0.35 : 0.75}
      />
    </g>
  )
}

/** Where the internet comes from: the fiber, or (mesh agent) the main router. */
function Upstream({
  y,
  uplink,
  beyond,
  internet,
  los,
  rate,
}: {
  y: number
  uplink?: MeshNode
  beyond: number
  internet: boolean
  los: boolean
  rate?: number
}) {
  const R = 34
  if (uplink) {
    const sig = uplink.link?.signal ?? null
    const wifi = !/^LAN|Ethernet/i.test(uplink.via)
    return (
      <g>
        <Edge x1={UP_X + R} y1={y} x2={RT_X - ROUTER_R} y2={y} wifi={wifi} />
        <g transform={`translate(${(UP_X + R + RT_X - ROUTER_R) / 2} ${y - 26})`}>
          <text textAnchor="middle" className="fill-muted-foreground text-[11px] font-medium">
            {uplink.via}
          </text>
          {sig != null && (
            <g transform="translate(0 14)">
              <title>{`Signal ${sig} dBm`}</title>
              <SignalBars x={-26} y={1} level={signalLevel(sig)} />
              <text x={6} textAnchor="middle" y={1} className="num fill-foreground text-[10.5px]">
                {sig} dBm
              </text>
            </g>
          )}
        </g>
        <circle cx={UP_X} cy={y} r={R} className="fill-card stroke-border" strokeWidth={1.5} />
        <RouterGlyph x={UP_X} y={y + 2} />
        <text x={UP_X} y={y + R + 18} textAnchor="middle" className="fill-foreground text-[12px] font-semibold">
          {truncate((uplink.name ?? 'Main router').split(' · ')[0], 18)}
        </text>
        <text x={UP_X} y={y + R + 33} textAnchor="middle" className="fill-muted-foreground text-[11px]">
          {uplink.name?.includes(' · ') ? `${uplink.name.split(' · ')[1]} · main router` : 'Main router'}
        </text>
        {beyond > 0 && (
          <g transform={`translate(${UP_X} ${y + R + 52})`}>
            <title>Devices on the main router's network, seen through this router's Wi-Fi backhaul</title>
            <circle cx={-plural(beyond, 'device').length * 3.1 - 8} cy={-4} r={3} className="fill-emerald-500" />
            <text textAnchor="middle" x={4} className="fill-muted-foreground text-[11px]">
              {plural(beyond, 'device')}
            </text>
          </g>
        )}
      </g>
    )
  }
  const state = los ? 'No light on the fiber' : internet ? 'Online' : 'Not connected'
  return (
    <g>
      <title>{`Internet: ${state}`}</title>
      <path
        d={`M ${UP_X + R} ${y} L ${RT_X - ROUTER_R} ${y}`}
        className={internet ? '[stroke:var(--mesh)]' : 'stroke-muted-foreground'}
        strokeWidth={3}
        strokeDasharray={internet ? undefined : '2 6'}
        strokeLinecap="round"
        opacity={internet ? 0.75 : 0.5}
      />
      <text
        x={(UP_X + R + RT_X - ROUTER_R) / 2}
        y={y - 12}
        textAnchor="middle"
        className="fill-muted-foreground text-[11px] font-medium"
      >
        Fiber
      </text>
      {internet && (rate ?? 0) > 1000 && (
        <text
          x={(UP_X + R + RT_X - ROUTER_R) / 2}
          y={y + 20}
          textAnchor="middle"
          className="num fill-muted-foreground text-[10.5px]"
        >
          {bitrate(rate!)}
        </text>
      )}
      <circle cx={UP_X} cy={y} r={R} className="fill-card stroke-border" strokeWidth={1.5} />
      <GlobeGlyph x={UP_X} y={y} />
      <text x={UP_X} y={y + R + 18} textAnchor="middle" className="fill-foreground text-[12px] font-semibold">
        Internet
      </text>
      <g transform={`translate(${UP_X} ${y + R + 32})`}>
        <circle
          cx={-(state.length * 2.9) - 8}
          cy={-4}
          r={3.5}
          className={los ? 'fill-red-500' : internet ? 'fill-emerald-500' : 'fill-muted-foreground/50'}
        />
        <text textAnchor="middle" x={4} className="fill-muted-foreground text-[11px]">
          {state}
        </text>
      </g>
    </g>
  )
}

/** A branch (Ethernet, a radio, a mesh device) and its device chips. */
function Branch({ g, cy, cap }: { g: Group; cy: number; cap: number }) {
  const shown = g.devices.slice(0, cap)
  const more = g.devices.length - shown.length
  const chipsH = Math.ceil((shown.length + (more > 0 ? 1 : 0)) / CHIP_COLS) * ROW - CHIP_GAP
  const chipTop = cy - chipsH / 2
  const at = (i: number) => ({
    x: CHIP_X + (i % CHIP_COLS) * (CHIP_W + CHIP_GAP),
    y: chipTop + Math.floor(i / CHIP_COLS) * ROW,
  })
  const bx = BR_X - BR_W / 2
  return (
    <g>
      {/* Branch → chips (only to the first chip of each row, to keep it calm) */}
      {shown.map((d, i) =>
        i % CHIP_COLS === 0 ? (
          <Edge key={`l-${d.mac}`} x1={bx + BR_W} y1={cy} x2={at(i).x} y2={at(i).y + CHIP_H / 2} wifi={g.wifi} dim />
        ) : null,
      )}

      <g transform={`translate(${bx} ${cy - BR_H / 2})`}>
        <rect width={BR_W} height={BR_H} rx={12} className="fill-card stroke-border" strokeWidth={1.5} />
        <circle cx={24} cy={BR_H / 2} r={15} className="[fill:var(--mesh)] [fill-opacity:0.15]" />
        {g.kind === 'ethernet' ? (
          <PortGlyph x={24} y={BR_H / 2} />
        ) : g.kind === 'wifi' ? (
          <WifiGlyph x={24} y={BR_H / 2 + 4} />
        ) : (
          <RouterGlyph x={24} y={BR_H / 2 + 2} small />
        )}
        <text x={47} y={21} className="fill-foreground text-[12.5px] font-semibold">
          {truncate(g.title, 19)}
        </text>
        {g.kind === 'ethernet' && g.ports?.length ? (
          <g transform="translate(47 35)">
            {g.ports.map((p, i) => (
              <g key={p.label}>
                <title>{`${p.label}: ${p.link ? (p.speedMbps === 1000 ? '1 Gbit/s' : `${p.speedMbps} Mbit/s`) : 'no cable'}`}</title>
                <rect
                  x={i * 13}
                  y={-4}
                  width={10}
                  height={8}
                  rx={2}
                  className={p.link ? 'fill-emerald-500' : 'fill-muted-foreground/25'}
                />
              </g>
            ))}
            <text x={g.ports.length * 13 + 4} y={3.5} className="fill-muted-foreground text-[11px]">
              {g.ports.filter((p) => p.link).length}/{g.ports.length} in use
            </text>
          </g>
        ) : (
          <text x={47} y={38} className="fill-muted-foreground text-[11px]">
            {g.sub}
            {(g.rate ?? 0) > 1000 && <tspan className="num fill-foreground/80"> · {bitrate(g.rate!)}</tspan>}
          </text>
        )}
      </g>

      {shown.map((d, i) => (
        <Chip key={d.mac} d={d} x={at(i).x} y={at(i).y} />
      ))}
      {more > 0 && (
        <g transform={`translate(${at(shown.length).x} ${at(shown.length).y})`}>
          <rect
            width={CHIP_W}
            height={CHIP_H}
            rx={CHIP_H / 2}
            className="fill-transparent stroke-border"
            strokeDasharray="3 3"
          />
          <text x={CHIP_W / 2} y={17} textAnchor="middle" className="fill-muted-foreground text-[11px] font-medium">
            +{more} more
          </text>
        </g>
      )}
      {!g.devices.length && (
        <text x={CHIP_X} y={cy + 4} className="fill-muted-foreground text-[11.5px] italic">
          {g.empty}
        </text>
      )}
    </g>
  )
}

function Chip({ d, x, y }: { d: Device; x: number; y: number }) {
  const lvl = d.signal != null ? signalLevel(d.signal) : null
  return (
    <g className="net-chip" transform={`translate(${x} ${y})`}>
      <title>{[d.name, d.ip, d.mac, d.signal != null && `${d.signal} dBm`].filter(Boolean).join(' · ')}</title>
      <rect width={CHIP_W} height={CHIP_H} rx={CHIP_H / 2} className="fill-card stroke-border" />
      <circle cx={13} cy={CHIP_H / 2} r={3.5} className="fill-emerald-500" />
      <text x={23} y={17} className="fill-foreground text-[11.5px] font-medium">
        {truncate(d.name, lvl != null ? 14 : 17)}
      </text>
      {lvl != null && <SignalBars x={CHIP_W - 14} y={CHIP_H / 2 + 4} level={lvl} />}
    </g>
  )
}

function truncate(s: string, n: number) {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s
}

function GlobeGlyph({ x, y }: { x: number; y: number }) {
  return (
    <g
      transform={`translate(${x} ${y})`}
      className="[stroke:var(--mesh)]"
      fill="none"
      strokeWidth={1.8}
      strokeLinecap="round"
    >
      <circle r={13} />
      <ellipse rx={5.5} ry={13} />
      <line x1={-13} y1={0} x2={13} y2={0} />
      <path d="M -11 -7 Q 0 -4 11 -7 M -11 7 Q 0 4 11 7" />
    </g>
  )
}

function WifiGlyph({ x, y }: { x: number; y: number }) {
  return (
    <g
      transform={`translate(${x} ${y})`}
      className="[stroke:var(--mesh)]"
      fill="none"
      strokeWidth={2}
      strokeLinecap="round"
    >
      <path d="M -9 -6 Q 0 -14 9 -6" />
      <path d="M -5.5 -2.5 Q 0 -7 5.5 -2.5" />
      <circle cx={0} cy={1.5} r={1.3} className="[fill:var(--mesh)]" stroke="none" />
    </g>
  )
}

function PortGlyph({ x, y }: { x: number; y: number }) {
  return (
    <g
      transform={`translate(${x} ${y})`}
      className="[stroke:var(--mesh)]"
      fill="none"
      strokeWidth={1.8}
      strokeLinejoin="round"
    >
      <path d="M -8 -6 H 8 V 4 H 4 V 7 H -4 V 4 H -8 Z" />
      <line x1={-3.5} y1={-6} x2={-3.5} y2={-2.5} />
      <line x1={0} y1={-6} x2={0} y2={-2.5} />
      <line x1={3.5} y1={-6} x2={3.5} y2={-2.5} />
    </g>
  )
}
