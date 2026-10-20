/** Two-series throughput sparkline (rx filled, tx line), scaled to the shared max. */
export function Sparkline({
  a, b, height = 44, label,
}: { a: number[]; b?: number[]; height?: number; label: string }) {
  const w = 240
  const n = Math.max(a.length, b?.length ?? 0, 2)
  const max = Math.max(1, ...a, ...(b ?? []))
  const pts = (s: number[]) =>
    s.map((v, i) => `${((i + (n - s.length)) / (n - 1)) * w},${height - (v / max) * (height - 3) - 1.5}`).join(' ')
  return (
    <svg viewBox={`0 0 ${w} ${height}`} preserveAspectRatio="none" className="h-11 w-full" role="img" aria-label={label}>
      {a.length > 1 && (
        <>
          <polygon points={`${((n - a.length) / (n - 1)) * w},${height} ${pts(a)} ${w},${height}`} fill="currentColor" opacity="0.14" />
          <polyline points={pts(a)} fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
        </>
      )}
      {b && b.length > 1 && (
        <polyline points={pts(b)} fill="none" stroke="var(--link)" strokeWidth="1.5" strokeDasharray="3 2" vectorEffect="non-scaling-stroke" />
      )}
    </svg>
  )
}
