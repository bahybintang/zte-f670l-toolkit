import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { Button } from '@/components/ui/button'
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from '@/components/ui/alert-dialog'

/** Saved changes the firmware only picks up at boot. */
export function RestartBanner({ titles }: { titles: string[] }) {
  const qc = useQueryClient()
  const [ask, setAsk] = useState(false)
  if (!titles.length) return null
  const restart = async () => {
    try {
      const r = await api.post<{ message: string }>('/action/reboot', { confirm: 'REBOOT' })
      toast.success(r.message)
      setTimeout(() => qc.invalidateQueries(), 100_000)
    } catch (e) {
      toast.error((e as Error).message)
    }
  }
  const list = titles.length > 3 ? `${titles.slice(0, 3).join(', ')} and ${titles.length - 3} more` : titles.join(', ')
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-led-warn/40 bg-led-warn/10 px-4 py-2 text-sm sm:px-6">
      <span className="led" data-state="warn" />
      <span className="min-w-0 flex-1">Saved and waiting for a restart: {list}.</span>
      <Button size="sm" variant="outline" onClick={() => setAsk(true)}>Restart now</Button>
      <AlertDialog open={ask} onOpenChange={setAsk}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Restart the router?</AlertDialogTitle>
            <AlertDialogDescription>
              Internet and Wi-Fi drop for about 90 seconds. This page reconnects on its own.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={restart}>Restart</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
