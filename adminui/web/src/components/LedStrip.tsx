import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

export interface Led { id: string; state: 'ok' | 'warn' | 'err' | 'off' | 'blink'; detail?: string }

export function useLeds() {
  return useQuery({
    queryKey: ['leds'],
    queryFn: async () => (await api.get<{ leds: Led[] }>('/status/leds')).leds,
    refetchInterval: 5_000,
  })
}

/** The router's front panel, live. Labels match the printing on the case. */
export function LedStrip({ size = 'sm' }: { size?: 'sm' | 'lg' }) {
  const { data } = useLeds()
  const leds = data ?? PLACEHOLDER
  const big = size === 'lg'
  return (
    <ul
      aria-label="Front panel indicators"
      className={
        big
          ? 'grid grid-cols-5 gap-x-6 gap-y-5 sm:grid-cols-10'
          : 'flex items-center gap-3.5 overflow-x-auto [scrollbar-width:none]'
      }
    >
      {leds.map((l) => (
        <li key={l.id}>
          <Tooltip>
            <TooltipTrigger
              render={<span />}
              className={
                big
                  ? 'flex flex-col items-center gap-2 outline-none'
                  : 'flex items-center gap-1.5 outline-none'
              }
              tabIndex={0}
            >
              <span className="led" data-state={l.state} style={big ? { width: 12, height: 12 } : undefined} />
              <span
                className={
                  big
                    ? 'font-heading text-[13px] font-medium text-muted-foreground'
                    : 'font-heading text-[12px] font-medium text-muted-foreground'
                }
              >
                {l.id}
              </span>
            </TooltipTrigger>
            <TooltipContent>
              {l.id}: {l.detail || STATE_TEXT[l.state]}
            </TooltipContent>
          </Tooltip>
        </li>
      ))}
    </ul>
  )
}

const STATE_TEXT: Record<Led['state'], string> = {
  ok: 'on',
  warn: 'attention',
  err: 'fault',
  off: 'off',
  blink: 'activity',
}

const PLACEHOLDER: Led[] = ['PON', 'LOS', 'Internet', 'LAN1', 'LAN2', 'LAN3', 'LAN4', '2.4G', '5G', 'USB'].map((id) => ({
  id,
  state: 'off',
}))
