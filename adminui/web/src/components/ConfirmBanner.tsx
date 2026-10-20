import { useEffect, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { pending, usePending, type PendingConfirm } from '@/lib/pending'
import { Button } from '@/components/ui/button'

/** Changes that could cut you off revert themselves unless you keep them. */
export function ConfirmBanner() {
  const items = usePending()
  if (!items.length) return null
  return (
    <div className="sticky top-14 z-10">
      {items.map((p) => (
        <Row key={p.token} p={p} />
      ))}
    </div>
  )
}

function Row({ p }: { p: PendingConfirm }) {
  const qc = useQueryClient()
  const [now, setNow] = useState(() => Date.now())
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(t)
  }, [])
  const left = Math.max(0, Math.round((p.deadline - now) / 1000))
  useEffect(() => {
    if (left === 0) {
      pending.remove(p.token)
      toast.warning(`“${p.title}” was reverted because it was not kept in time.`)
      qc.invalidateQueries({ queryKey: ['page'] })
    }
  }, [left, p, qc])

  const keep = async () => {
    setBusy(true)
    try {
      await api.post(`/confirm/${p.token}`)
      pending.remove(p.token)
      toast.success('Change kept.')
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const revert = async () => {
    setBusy(true)
    try {
      await api.post(`/history/${p.entry}/undo`)
      pending.remove(p.token)
      qc.invalidateQueries({ queryKey: ['page'] })
      toast.success('Change reverted.')
    } catch (e) {
      toast.error((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const m = Math.floor(left / 60)
  const s = String(left % 60).padStart(2, '0')
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-fiber/60 bg-fiber/18 px-4 py-2.5 text-sm sm:px-6">
      <span className="num font-heading text-base font-semibold tabular-nums">{m}:{s}</span>
      <span className="min-w-0 flex-1">
        <b className="font-medium">{p.title}</b> is applied. If this page still works for you, keep it — otherwise it
        reverts on its own.
      </span>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={busy} onClick={revert}>
          Revert now
        </Button>
        <Button size="sm" disabled={busy} onClick={keep}>
          Keep change
        </Button>
      </div>
    </div>
  )
}
