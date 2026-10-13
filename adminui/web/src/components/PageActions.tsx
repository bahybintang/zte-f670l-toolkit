import { useState } from 'react'
import { toast } from 'sonner'
import { useNavigate } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Action, Page } from '@/lib/schema'
import { Button } from '@/components/ui/button'
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from '@/components/ui/alert-dialog'

// Server endpoints that exist today, by schema action id.
const ENDPOINTS: Record<string, { path: string; confirmWord?: string }> = {
  reboot: { path: '/action/reboot', confirmWord: 'REBOOT' },
  restart: { path: '/action/reboot', confirmWord: 'REBOOT' },
  'tr069-inform': { path: '/action/diag/tr069-inform' },
  'voice-line-test': { path: '/action/diag/voice-line-test' },
  'voice-call-test': { path: '/action/diag/voice-call-test' },
  'simulation-start': { path: '/action/diag/simulation-start' },
  'simulation-stop': { path: '/action/diag/simulation-stop' },
}

// ping/traceroute run live from the Network tools page (they take a target).
const TOOLS_ACTIONS = new Set(['ping', 'traceroute', 'traceroute-stop'])

export function PageActions({ page }: { page: Page }) {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const [ask, setAsk] = useState<Action | null>(null)
  const actions = (page.actions ?? []).filter((a) => a.id !== 'refresh')
  const hasRefresh = page.actions?.some((a) => a.id === 'refresh')
  const run = async (a: Action) => {
    if (TOOLS_ACTIONS.has(a.id)) {
      navigate({ to: '/tools' })
      return
    }
    const ep = ENDPOINTS[a.id]
    if (!ep) return
    try {
      const r = await api.post<{ message?: string }>(ep.path, ep.confirmWord ? { confirm: ep.confirmWord } : {})
      toast.success(r.message ?? `${a.label}: done.`)
    } catch (e) {
      toast.error((e as Error).message)
    }
  }
  if (!actions.length && !hasRefresh) return null
  return (
    <div className="mt-10 flex flex-wrap gap-2 border-t border-border pt-5">
      {hasRefresh && (
        <Button variant="outline" onClick={() => qc.invalidateQueries({ queryKey: ['page', page.id] })}>Refresh</Button>
      )}
      {actions.map((a) => {
        const available = !!ENDPOINTS[a.id] || TOOLS_ACTIONS.has(a.id)
        return (
          <Button
            key={a.id}
            variant={a.danger && a.danger !== 'none' ? 'destructive' : 'outline'}
            disabled={!available}
            title={available ? (TOOLS_ACTIONS.has(a.id) ? 'Opens Network tools' : undefined) : 'Not available in this version yet'}
            onClick={() => (a.danger && a.danger !== 'none' ? setAsk(a) : run(a))}
          >
            {a.label}
          </Button>
        )
      })}
      <AlertDialog open={!!ask} onOpenChange={(o) => !o && setAsk(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{ask?.label}?</AlertDialogTitle>
            <AlertDialogDescription>The router may drop connections while this runs.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => ask && run(ask)}>{ask?.label}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
