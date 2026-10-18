import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Search } from 'lucide-react'
import { api } from '@/lib/api'
import { cn } from '@/lib/utils'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'

interface Host {
  MAC: string; HostName?: string; IP?: string; arpIp?: string; If?: string; Active?: string; AddrSrc?: string; LastConn?: string
}
interface WifiClient { mac: string; iface: string; band: string; [k: string]: string }

export function Clients() {
  const hosts = useQuery({
    queryKey: ['live', 'hosts'],
    queryFn: async () => (await api.get<{ data: { rows: Host[] } }>('/status/live/hosts')).data.rows,
    refetchInterval: 10_000,
  })
  const wifi = useQuery({
    queryKey: ['wifi-clients'],
    queryFn: async () => (await api.get<{ clients: WifiClient[] }>('/wifi/clients')).clients,
    refetchInterval: 10_000,
  })
  const [q, setQ] = useState('')
  const [onlineOnly, setOnlineOnly] = useState(false)

  const rows = useMemo(() => {
    // Be defensive: a stale cache or odd response could hand us a non-array.
    const hostList = Array.isArray(hosts.data) ? hosts.data : []
    const wifiList = Array.isArray(wifi.data) ? wifi.data : []
    const w = new Map(wifiList.map((c) => [c.mac, c]))
    return hostList
      .map((h) => ({ ...h, wifi: w.get(h.MAC), online: h.Active === '1' || w.has(h.MAC) }))
      .filter((h) => !onlineOnly || h.online)
      .filter((h) => !q || [h.HostName, h.MAC, h.IP, h.arpIp].some((x) => x?.toLowerCase().includes(q.toLowerCase())))
      .sort((a, b) => Number(b.online) - Number(a.online) || (a.HostName ?? '').localeCompare(b.HostName ?? ''))
  }, [hosts.data, wifi.data, q, onlineOnly])

  const online = rows.filter((r) => r.online).length
  return (
    <article>
      <header className="mb-6 max-w-2xl">
        <h1 className="font-heading text-[32px] font-semibold tracking-tight">Devices</h1>
        <p className="mt-1.5 text-muted-foreground">
          Everything the router has seen on your network. {hosts.data ? `${online} online now.` : ''}
        </p>
      </header>
      <div className="mb-4 flex flex-wrap items-center gap-4">
        <label className="flex h-9 w-full max-w-xs items-center gap-2 rounded-md border border-input bg-card px-2.5 text-sm focus-within:outline-2 focus-within:outline-ring">
          <Search className="size-4 text-muted-foreground" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Name, IP or MAC" className="min-w-0 flex-1 bg-transparent outline-none" />
        </label>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={onlineOnly} onChange={(e) => setOnlineOnly(e.target.checked)} className="accent-foreground" />
          Online only
        </label>
      </div>
      {hosts.isError && <p className="text-destructive">{(hosts.error as Error).message}</p>}
      {hosts.data && rows.length === 0 && <p className="text-muted-foreground">No devices match.</p>}
      <ul className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {rows.map((h) => {
          const ip = h.IP && h.IP !== '0.0.0.0' ? h.IP : h.arpIp
          const where = h.If?.startsWith('wlan') ? (Number(h.If.slice(4)) >= 4 ? 'Wi-Fi 5 GHz' : 'Wi-Fi 2.4 GHz') : h.If ? 'Ethernet' : ''
          return (
            <li key={h.MAC} className={cn('rounded-lg border border-border bg-card px-4 py-3', !h.online && 'opacity-70')}>
              <div className="flex items-center gap-2">
                <span className="led" data-state={h.online ? 'ok' : undefined} />
                <span className="truncate font-medium">{h.HostName || 'Unnamed device'}</span>
              </div>
              <dl className="num mt-2 grid grid-cols-[72px_1fr] gap-y-0.5 text-[13px]">
                <dt className="text-muted-foreground">IP</dt><dd>{ip || '–'}</dd>
                <dt className="text-muted-foreground">MAC</dt><dd>{h.MAC}</dd>
                <dt className="text-muted-foreground">Via</dt><dd>{where || '–'}{h.AddrSrc ? ` · ${h.AddrSrc}` : ''}</dd>
                {h.wifi && (
                  <>
                    <dt className="text-muted-foreground">Signal</dt>
                    <dd>{h.wifi.RSSI ?? h.wifi.RSSI0 ?? '–'} dBm</dd>
                  </>
                )}
                {!h.online && h.LastConn && (
                  <>
                    <dt className="text-muted-foreground">Last seen</dt><dd>{h.LastConn}</dd>
                  </>
                )}
              </dl>
              {!h.online && !h.If?.startsWith('wlan') && (
                <Button
                  variant="outline"
                  size="sm"
                  className="mt-3"
                  onClick={() =>
                    api.post('/tools/wol', { mac: h.MAC })
                      .then(() => toast.success(`Wake-up packet sent to ${h.HostName || h.MAC}.`))
                      .catch((e) => toast.error((e as Error).message))
                  }
                >
                  Wake
                </Button>
              )}
            </li>
          )
        })}
      </ul>
    </article>
  )
}
