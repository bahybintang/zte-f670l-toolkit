import { useQuery } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { api } from '@/lib/api'
import { bytes } from '@/lib/format'
import { Button } from '@/components/ui/button'
import { Notice } from './SchemaPage'

interface Part { index: number; size: number; name: string; booted: boolean }

const ROLE: Record<string, string> = {
  'whole flash': 'Everything below, in one image. Keep this one somewhere safe.',
  'u-boot': 'Bootloader',
  'parameter tags': 'Factory data: serial number, MAC addresses',
  kernel0: 'Kernel, bank A',
  kernel1: 'Kernel, bank B',
  usercfg: 'Your configuration',
  wlan: 'Wi-Fi calibration data',
  rootfs0: 'System, bank A',
  rootfs1: 'System, bank B',
  apps: 'Add-ons, including adminui',
}

export function Flash() {
  const q = useQuery({ queryKey: ['flash'], queryFn: () => api.get<{ partitions: Part[]; rootPartition?: number }>('/flash/partitions') })
  const bank = q.data?.rootPartition === 8 ? 'A' : q.data?.rootPartition === 9 ? 'B' : '?'
  return (
    <article>
      <header className="mb-6 max-w-2xl">
        <h1 className="font-heading text-[32px] font-semibold tracking-tight">Flash and firmware</h1>
        <p className="mt-1.5 text-muted-foreground">
          Download a copy of any part of the router’s flash. Reading is safe and changes nothing.
          {q.data && <> The router is running from system bank <b className="text-foreground">{bank}</b>.</>}
        </p>
      </header>
      <Notice tone="muted">A full image of a 256 MB flash takes a few minutes to download. Leave this page open until it finishes.</Notice>
      <div className="overflow-x-auto rounded-lg border border-border bg-card">
        <table className="w-full text-[14px]">
          <thead>
            <tr className="border-b border-border text-left text-[13px] text-muted-foreground">
              <th className="px-3 py-2 font-medium">Partition</th>
              <th className="px-3 py-2 font-medium">Holds</th>
              <th className="px-3 py-2 text-right font-medium">Size</th>
              <th className="w-32" />
            </tr>
          </thead>
          <tbody className="divide-y divide-border/70">
            {q.data?.partitions?.map((p) => (
              <tr key={p.index}>
                <td className="num px-3 py-2 whitespace-nowrap">
                  mtd{p.index} <span className="text-muted-foreground">{p.name}</span>
                  {p.booted && <span className="ml-2 rounded bg-led-ok/15 px-1.5 py-0.5 text-[12px]">running</span>}
                </td>
                <td className="px-3 py-2 text-muted-foreground">{ROLE[p.name] ?? ''}</td>
                <td className="num px-3 py-2 text-right whitespace-nowrap">{bytes(p.size)}</td>
                <td className="px-2 py-1 text-right">
                  <Button variant="ghost" size="sm" render={<a href={`/api/flash/dump/${p.index}`} download />}>
                    <Download /> Download
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </article>
  )
}
