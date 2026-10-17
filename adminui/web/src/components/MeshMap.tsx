export interface MeshNode {
  almac: string
  via: string
  radios: string[]
  clients: number
}

const W = 640
const NODE_W = 196
const NODE_H = 60
const ROOT_R = 46

/** Canvas height and agent positions for n agents around the controller. */
function layout(n: number) {
  if (n === 0) {
    const H = 200
    return { H, root: { x: W / 2, y: 86 }, at: () => ({ x: 0, y: 0 }) }
  }
  if (n === 1) {
    const H = 210
    return { H, root: { x: W / 2 - 130, y: H / 2 }, at: () => ({ x: W / 2 + 150, y: H / 2 }) }
  }
  if (n === 2) {
    const H = 230
    return { H, root: { x: W / 2, y: H / 2 }, at: (i: number) => ({ x: i === 0 ? W / 2 - 205 : W / 2 + 205, y: H / 2 }) }
  }
  // Ring around the controller, then fit the canvas to what was placed.
  const pts = Array.from({ length: n }, (_, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / n
    return { x: W / 2 + Math.cos(a) * 222, y: Math.sin(a) * 175 }
  })
  const top = Math.min(...pts.map((p) => p.y), -ROOT_R) - NODE_H / 2 - 18
  const bottom = Math.max(...pts.map((p) => p.y), ROOT_R + 24) + NODE_H / 2 + 18
  const root = { x: W / 2, y: -top }
  return { H: bottom - top, root, at: (i: number) => ({ x: pts[i].x, y: pts[i].y - top }) }
}

const isWifi = (via: string) => !/^LAN|Ethernet/i.test(via)
const bands = (radios: string[]) =>
  [...new Set(radios.map((r) => (r.includes('2.4') ? '2.4' : r.includes('60') ? '60' : '5')))].join(' + ') + ' GHz'

/** Mesh topology: the controller in the centre, mesh devices around it. */
export function MeshMap({ nodes, role }: { nodes: MeshNode[]; role: 'controller' | 'agent' }) {
  const { H, root, at } = layout(nodes.length)
  const CX = root.x
  const CY = root.y
  return (
    <figure className="mesh-map m-0">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="h-auto w-full"
        role="img"
        aria-label={`Mesh map: this router and ${nodes.length} other mesh device${nodes.length === 1 ? '' : 's'}`}
      >
        <defs>
          <radialGradient id="mesh-glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" className="[stop-color:var(--mesh)]" stopOpacity="0.28" />
            <stop offset="100%" className="[stop-color:var(--mesh)]" stopOpacity="0" />
          </radialGradient>
          <pattern id="mesh-grid" width="24" height="24" patternUnits="userSpaceOnUse">
            <circle cx="1" cy="1" r="1" className="fill-border" />
          </pattern>
        </defs>
        <rect width={W} height={H} rx="12" fill="url(#mesh-grid)" />

        {nodes.map((n, i) => {
          const p = at(i)
          const wifi = isWifi(n.via)
          // Label at the middle of the link.
          const mx = (CX + p.x) / 2
          const my = (CY + p.y) / 2
          return (
            <g key={`link-${n.almac}`}>
              <line
                x1={CX}
                y1={CY}
                x2={p.x}
                y2={p.y}
                className={wifi ? 'mesh-wifi [stroke:var(--mesh)]' : '[stroke:var(--mesh)]'}
                strokeWidth={wifi ? 2 : 3}
                strokeDasharray={wifi ? '6 6' : undefined}
                strokeLinecap="round"
                opacity={0.75}
              />
              <g transform={`translate(${mx} ${my})`}>
                <rect x={-30} y={-11} width={60} height={22} rx={11} className="fill-card stroke-border" />
                <text textAnchor="middle" dy="4" className="fill-muted-foreground text-[11px] font-medium">
                  {n.via}
                </text>
              </g>
            </g>
          )
        })}

        <circle cx={CX} cy={CY} r={ROOT_R * 2.2} fill="url(#mesh-glow)" />
        <circle cx={CX} cy={CY} r={ROOT_R} className="fill-card [stroke:var(--mesh)]" strokeWidth={2.5} />
        <RouterGlyph x={CX} y={CY - 10} />
        <text x={CX} y={CY + 18} textAnchor="middle" className="fill-foreground text-[12px] font-semibold">
          This router
        </text>
        <text x={CX} y={CY + ROOT_R + 18} textAnchor="middle" className="fill-muted-foreground text-[11px]">
          {role === 'controller' ? 'Controller' : 'Agent'}
        </text>

        {nodes.map((n, i) => {
          const p = at(i)
          return (
            <g key={n.almac} transform={`translate(${p.x - NODE_W / 2} ${p.y - NODE_H / 2})`}>
              <rect width={NODE_W} height={NODE_H} rx={12} className="fill-card stroke-border" strokeWidth={1.5} />
              <circle cx={22} cy={NODE_H / 2} r={14} className="[fill:var(--mesh)] [fill-opacity:0.15]" />
              <RouterGlyph x={22} y={NODE_H / 2 + 2} small />
              <text x={44} y={25} className="fill-foreground font-mono text-[11.5px]">
                {n.almac}
              </text>
              <text x={44} y={44} className="fill-muted-foreground text-[11px]">
                {n.radios.length ? bands(n.radios) : 'Mesh device'}
              </text>
              <Clients x={NODE_W - 14} y={40} count={n.clients} />
            </g>
          )
        })}

        {!nodes.length && (
          <text x={CX} y={CY + ROOT_R + 44} textAnchor="middle" className="fill-muted-foreground text-[12px]">
            No other mesh devices yet
          </text>
        )}
      </svg>
      <style>{`
        .mesh-map { --mesh: #b8860b; }
        .dark .mesh-map { --mesh: var(--color-fiber); }
        .mesh-wifi { animation: mesh-flow 1.2s linear infinite; }
        @keyframes mesh-flow { to { stroke-dashoffset: -12; } }
        @media (prefers-reduced-motion: reduce) { .mesh-wifi { animation: none; } }
      `}</style>
      <figcaption className="mt-2 flex flex-wrap gap-x-5 gap-y-1 text-[12px] text-muted-foreground">
        <span className="inline-flex items-center gap-1.5">
          <svg width="22" height="6" aria-hidden><line x1="1" y1="3" x2="21" y2="3" className="[stroke:var(--mesh)]" strokeWidth="3" strokeLinecap="round" /></svg>
          Cable
        </span>
        <span className="inline-flex items-center gap-1.5">
          <svg width="22" height="6" aria-hidden><line x1="1" y1="3" x2="21" y2="3" className="[stroke:var(--mesh)]" strokeWidth="2" strokeDasharray="4 4" /></svg>
          Wi-Fi
        </span>
        <span className="inline-flex items-center gap-1.5">
          <svg width="10" height="10" aria-hidden><circle cx="5" cy="5" r="4" className="fill-emerald-500" /></svg>
          Connected clients
        </span>
      </figcaption>
    </figure>
  )
}

/** A small router: box with two antennas. */
function RouterGlyph({ x, y, small }: { x: number; y: number; small?: boolean }) {
  const s = small ? 0.7 : 1
  return (
    <g transform={`translate(${x} ${y}) scale(${s})`} className="[stroke:var(--mesh)]" fill="none" strokeWidth={2} strokeLinecap="round">
      <rect x={-11} y={-2} width={22} height={9} rx={2.5} />
      <line x1={-6} y1={-2} x2={-8} y2={-10} />
      <line x1={6} y1={-2} x2={8} y2={-10} />
      <circle cx={-5} cy={2.5} r={0.8} className="[fill:var(--mesh)]" />
      <circle cx={0} cy={2.5} r={0.8} className="[fill:var(--mesh)]" />
    </g>
  )
}

/** Client count: up to four dots and the number, right-aligned at (x, y). */
function Clients({ x, y, count }: { x: number; y: number; count: number }) {
  const dots = Math.min(count, 4)
  return (
    <g transform={`translate(${x} ${y})`}>
      <title>{count === 0 ? 'No clients' : `${count} connected client${count === 1 ? '' : 's'}`}</title>
      <text textAnchor="end" y={4} className="fill-muted-foreground text-[11px] font-medium">
        {count}
      </text>
      {Array.from({ length: dots }, (_, i) => (
        <circle key={i} cx={-14 - i * 8} cy={0} r={3} className="fill-emerald-500" opacity={1 - i * 0.15} />
      ))}
      {count === 0 && <circle cx={-14} cy={0} r={3} className="fill-none stroke-muted-foreground" strokeWidth={1} />}
    </g>
  )
}
