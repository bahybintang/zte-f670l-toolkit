import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { api } from '@/lib/api'
import { useSystem, useTraffic } from '@/lib/queries'
import { bitrate, bytes, duration } from '@/lib/format'
import { LedStrip } from '@/components/LedStrip'
import { Sparkline } from '@/components/Sparkline'

interface Optics { temperatureC?: number; supplyVoltageV?: number; biasCurrentMa?: number; txPowerDbm?: number | null; rxPowerDbm?: number | null; los?: boolean }
interface Port { label: string; link: boolean; speedMbps?: number | null; rxBytes?: number; txBytes?: number }

function useLive<T>(name: string, ms = 5000) {
  return useQuery({
    queryKey: ['live', name],
    queryFn: async () => (await api.get<{ data: T }>(`/status/live/${name}`)).data,
    refetchInterval: ms,
  })
}

const IFACES: { id: string; label: string }[] = [
  { id: 'br0', label: 'LAN bridge' },
  { id: 'wlan0', label: 'Wi-Fi 2.4 GHz' },
  { id: 'wlan4', label: 'Wi-Fi 5 GHz' },
  { id: 'pon0', label: 'Fiber (WAN)' },
]

export function Dashboard() {
  const sys = useSystem()
  const traffic = useTraffic()
  const optics = useLive<Optics>('pon_optics')
  const ports = useLive<{ rows: Port[] }>('lan_ports')
  const hosts = useLive<{ rows: { Active?: string }[] }>('hosts', 15000)
  const s = sys.data
  const memUsed = s ? 1 - s.memAvailableKb / s.memTotalKb : 0
  const cpu = traffic.data?.cpu ?? []
  const online = hosts.data?.rows.filter((h) => h.Active === '1').length

  return (
    <div>
      {/* The front panel, as printed on the case. */}
      <section aria-labelledby="panel" className="mb-10 rounded-[14px] border border-border bg-card px-6 pt-5 pb-6 shadow-[0_1px_0_var(--border),0_24px_48px_-32px_rgb(22_32_46/0.35)]">
        <div className="mb-6 flex flex-wrap items-baseline justify-between gap-2">
          <h1 id="panel" className="font-heading text-[30px] font-semibold tracking-tight">
            {s?.ModelName || 'F670L'}
          </h1>
          <p className="num text-sm text-muted-foreground">
            {s ? <>Up {duration(s.uptimeSecs)} · firmware {s.SoftwareVer ?? '–'}</> : 'Reading device…'}
          </p>
        </div>
        <LedStrip size="lg" />
      </section>

      <div className="grid gap-x-10 gap-y-10 lg:grid-cols-[1.25fr_1fr]">
        <section aria-labelledby="tp">
          <h2 id="tp" className="mb-3 border-b border-border pb-2 font-heading text-[19px] font-semibold">Throughput</h2>
          <ul className="divide-y divide-border/70">
            {IFACES.map((i) => {
              const t = traffic.data?.ifaces.find((x) => x.name === i.id)
              const rx = t?.rxRate.at(-1) ?? 0
              const tx = t?.txRate.at(-1) ?? 0
              return (
                <li key={i.id} className="grid grid-cols-[130px_1fr] items-center gap-4 py-2.5 sm:grid-cols-[150px_1fr_150px]">
                  <div>
                    <div className="text-[14.5px] font-medium">{i.label}</div>
                    <div className="num text-[12.5px] text-muted-foreground">{t ? `${bytes(t.rxBytes)} in · ${bytes(t.txBytes)} out` : i.id}</div>
                  </div>
                  <div className="text-foreground/80">
                    <Sparkline a={t?.rxRate ?? []} b={t?.txRate ?? []} label={`${i.label} throughput, last 3 minutes`} />
                  </div>
                  <div className="num col-span-2 text-[13px] sm:col-span-1 sm:text-right">
                    <div>↓ {bitrate(rx)}</div>
                    <div className="text-link">↑ {bitrate(tx)}</div>
                  </div>
                </li>
              )
            })}
          </ul>
        </section>

        <section aria-labelledby="fiber">
          <h2 id="fiber" className="mb-3 border-b border-border pb-2 font-heading text-[19px] font-semibold">Fiber optics</h2>
          {optics.data?.los && (
            <p className="mb-3 flex items-center gap-2 text-sm">
              <span className="led" data-state="err" /> No light received. Check that the fiber is plugged in and the connector is clean.
            </p>
          )}
          <dl className="grid grid-cols-2 gap-x-6 gap-y-4">
            <Metric label="Received power" value={fmtDbm(optics.data?.rxPowerDbm)} hint="Good: −8 to −27 dBm" />
            <Metric label="Transmit power" value={fmtDbm(optics.data?.txPowerDbm)} hint="Typical: +0.5 to +5 dBm" />
            <Metric label="Module temperature" value={optics.data?.temperatureC != null ? `${optics.data.temperatureC} °C` : '–'} />
            <Metric label="Laser bias" value={optics.data?.biasCurrentMa != null ? `${optics.data.biasCurrentMa} mA` : '–'} />
          </dl>
        </section>

        <section aria-labelledby="lan">
          <div className="mb-3 flex items-baseline justify-between border-b border-border pb-2">
            <h2 id="lan" className="font-heading text-[19px] font-semibold">Ethernet</h2>
            <Link to="/clients" className="text-sm text-link hover:underline">
              {online !== undefined ? `${online} ${online === 1 ? 'device' : 'devices'} online` : 'Devices'}
            </Link>
          </div>
          <ul className="grid grid-cols-4 gap-3">
            {(ports.data?.rows ?? [0, 1, 2, 3].map((n) => ({ label: `LAN${n + 1}`, link: false }) as Port)).map((p) => (
              <li key={p.label} className="rounded-lg border border-border bg-card px-3 py-2.5">
                <div className="flex items-center gap-2">
                  <span className="led" data-state={p.link ? 'ok' : undefined} />
                  <span className="font-heading text-[15px] font-semibold">{p.label}</span>
                </div>
                <div className="num mt-1 text-[13px] text-muted-foreground">
                  {p.link ? (p.speedMbps === 1000 ? '1 Gbit/s' : `${p.speedMbps} Mbit/s`) : 'No cable'}
                </div>
              </li>
            ))}
          </ul>
        </section>

        <section aria-labelledby="sys">
          <h2 id="sys" className="mb-3 border-b border-border pb-2 font-heading text-[19px] font-semibold">System</h2>
          <dl className="grid grid-cols-2 gap-x-6 gap-y-4">
            <div>
              <dt className="text-[13px] text-muted-foreground">Processor</dt>
              <dd className="num font-heading text-[24px] font-semibold">{cpu.length ? `${Math.round(cpu.at(-1)!)}%` : '–'}</dd>
              <div className="text-foreground/70">
                <Sparkline a={cpu} label="CPU use, last 3 minutes" />
              </div>
            </div>
            <div>
              <dt className="text-[13px] text-muted-foreground">Memory in use</dt>
              <dd className="num font-heading text-[24px] font-semibold">{s ? `${Math.round(memUsed * 100)}%` : '–'}</dd>
              <div className="mt-3 h-2 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full bg-foreground/70" style={{ width: `${memUsed * 100}%` }} />
              </div>
              <div className="num mt-1.5 text-[12.5px] text-muted-foreground">
                {s ? `${bytes((s.memTotalKb - s.memAvailableKb) * 1024)} of ${bytes(s.memTotalKb * 1024)}` : ''}
              </div>
            </div>
          </dl>
        </section>
      </div>
    </div>
  )
}

function fmtDbm(v?: number | null) {
  if (v === null || v === undefined) return '–'
  return `${v > 0 ? '+' : v < 0 ? '−' : ''}${Math.abs(v).toFixed(2)} dBm`
}

function Metric({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <dt className="text-[13px] text-muted-foreground">{label}</dt>
      <dd className="num font-heading text-[24px] font-semibold">{value}</dd>
      {hint && <div className="text-[12.5px] text-muted-foreground">{hint}</div>}
    </div>
  )
}
