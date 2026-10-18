import { useMemo, useState } from 'react'
import qrcode from 'qrcode-generator'
import { useQueries } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { SectionData } from '@/lib/schema'
import { Input } from '@/components/ui/input'

const ROWS = [0, 1, 2, 3, 4, 5, 6, 7]

// WIFI: URI escaping per the de-facto ZXing format.
const esc = (s: string) => s.replace(/([\;,:"])/g, '\\$1')

/** Join a phone to Wi-Fi by scanning. Built in the browser; nothing is sent anywhere. */
export function ShareWifi() {
  const ssids = useQueries({
    queries: ROWS.map((r) => ({
      queryKey: ['page', 'wlan-ssids', String(r)],
      queryFn: () => api.get<{ sections: Record<string, SectionData> }>(`/page/wlan-ssids?sel=${r}`),
    })),
  })
  const list = ROWS.map((r, i) => {
    const sec = Object.values(ssids[i].data?.sections ?? {}).find((s) => s.rows?.[0]?.values.ESSID !== undefined)
    const v = sec?.rows?.[0]?.values
    return v ? { row: r, ssid: v.ESSID, on: v.Enable === '1', hidden: v.ESSIDHideEnable === '1' } : null
  }).filter((x): x is NonNullable<typeof x> => !!x && x.on)
  const [pick, setPick] = useState<number | null>(null)
  const [pass, setPass] = useState('')
  const cur = list.find((x) => x.row === pick) ?? list[0]
  const svg = useMemo(() => {
    if (!cur) return ''
    const q = qrcode(0, 'M')
    q.addData(`WIFI:T:${pass ? 'WPA' : 'nopass'};S:${esc(cur.ssid)};${pass ? `P:${esc(pass)};` : ''}${cur.hidden ? 'H:true;' : ''};`)
    q.make()
    return q.createSvgTag({ cellSize: 6, margin: 2, scalable: true })
  }, [cur, pass])

  return (
    <article className="max-w-3xl">
      <h1 className="font-heading text-[32px] font-semibold tracking-tight">Share Wi-Fi</h1>
      <p className="mt-1.5 mb-6 text-muted-foreground">
        Scan with a phone camera to join. The router never reveals saved passwords, so type it here; it stays in this page.
      </p>
      <div className="grid gap-8 sm:grid-cols-[1fr_260px]">
        <div className="space-y-4">
          <div className="flex flex-wrap gap-1.5">
            {list.map((x) => (
              <button
                key={x.row}
                onClick={() => setPick(x.row)}
                className={
                  cur?.row === x.row
                    ? 'rounded-md border border-foreground bg-foreground px-3 py-1.5 text-[14px] font-medium text-background'
                    : 'rounded-md border border-input bg-card px-3 py-1.5 text-[14px] hover:border-foreground/40'
                }
              >
                {x.ssid} <span className="opacity-70">· {x.row < 4 ? '2.4 GHz' : '5 GHz'}</span>
              </button>
            ))}
          </div>
          <label className="block space-y-1.5">
            <span className="text-sm font-medium">Wi-Fi password</span>
            <Input type="password" autoComplete="off" value={pass} onChange={(e) => setPass(e.target.value)} className="h-9 max-w-sm bg-card" placeholder="Leave empty for an open network" />
          </label>
        </div>
        <div className="rounded-xl border border-border bg-white p-3" aria-label="Wi-Fi QR code">
          {svg ? <div dangerouslySetInnerHTML={{ __html: svg }} /> : <p className="p-6 text-sm text-neutral-500">No enabled networks.</p>}
        </div>
      </div>
    </article>
  )
}
