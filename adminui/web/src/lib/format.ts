export function bytes(n: number) {
  if (!Number.isFinite(n)) return '–'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  while (n >= 1024 && i < u.length - 1) {
    n /= 1024
    i++
  }
  return `${n.toFixed(n >= 100 || i === 0 ? 0 : 1)} ${u[i]}`
}

export function bitrate(bytesPerSec: number) {
  const b = bytesPerSec * 8
  if (b < 1000) return `${b.toFixed(0)} bit/s`
  if (b < 1e6) return `${(b / 1e3).toFixed(1)} kbit/s`
  if (b < 1e9) return `${(b / 1e6).toFixed(1)} Mbit/s`
  return `${(b / 1e9).toFixed(2)} Gbit/s`
}

export function duration(secs: number) {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d) return `${d}d ${h}h`
  if (h) return `${h}h ${m}m`
  return `${m}m ${secs % 60}s`
}

/** The router has no RTC; without NTP its clock starts at 1970. */
export function when(ts: number) {
  if (ts < 946684800) return `${duration(ts)} after boot`
  return new Date(ts * 1000).toLocaleString()
}
