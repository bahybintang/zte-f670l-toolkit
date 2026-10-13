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

// Actions that download a file rather than POST + toast.
const DOWNLOADS: Record<string, string> = {
  'backup-download': '/api/backup/download',
  'log-download': '/api/log/download',
}

export function PageActions({ page }: { page: Page }) {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const [ask, setAsk] = useState<Action | null>(null)
  const [result, setResult] = useState<string | null>(null)
  const actions = (page.actions ?? []).filter((a) => a.id !== 'refresh')
  const hasRefresh = page.actions?.some((a) => a.id === 'refresh')
  const run = async (a: Action) => {
    if (TOOLS_ACTIONS.has(a.id)) {
      navigate({ to: '/tools' })
      return
    }
    const ep = ENDPOINTS[a.id]
    if (!ep) return
    setResult(null)
    try {
      const r = await api.post<{ message?: string }>(ep.path, ep.confirmWord ? { confirm: ep.confirmWord } : {})
      const msg = r.message ?? `${a.label}: done.`
      toast.success(msg)
      setResult(msg) // keep the outcome visible on the page, not just a transient toast
    } catch (e) {
      const msg = (e as Error).message
      toast.error(msg)
      setResult(msg)
    }
  }
  if (!actions.length && !hasRefresh) return null
  return (
    <div className="mt-10 border-t border-border pt-5">
      <div className="flex flex-wrap gap-2">
      {hasRefresh && (
        <Button variant="outline" onClick={() => qc.invalidateQueries({ queryKey: ['page', page.id] })}>Refresh</Button>
      )}
      {actions.map((a) => {
        const dl = DOWNLOADS[a.id]
        if (dl) {
          return (
            <Button key={a.id} variant="outline" render={<a href={dl} download />}>
              {a.label}
            </Button>
          )
        }
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
      </div>
      {result && (
        <p className="mt-4 rounded-lg border border-border bg-card px-4 py-3 text-sm text-muted-foreground">{result}</p>
      )}
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
