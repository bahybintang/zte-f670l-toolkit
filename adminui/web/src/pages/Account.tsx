import { useState, type FormEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { useSession } from '@/lib/queries'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

export function Account() {
  const qc = useQueryClient()
  const s = useSession()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [again, setAgain] = useState('')
  const [busy, setBusy] = useState(false)
  const mismatch = again !== '' && next !== again
  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    try {
      await api.post('/password', { current, new: next })
      toast.success('Password changed.')
      setCurrent(''); setNext(''); setAgain('')
      qc.invalidateQueries({ queryKey: ['session'] })
    } catch (err) {
      toast.error((err as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <article className="max-w-lg">
      <h1 className="font-heading text-[32px] font-semibold tracking-tight">Sign-in password</h1>
      <p className="mt-1.5 mb-6 text-muted-foreground">
        For this admin page only ({s.data?.user}). The router’s own web page and telnet keep their passwords.
      </p>
      <form onSubmit={submit} className="space-y-4">
        <div className="space-y-1.5">
          <Label htmlFor="cur">Current password</Label>
          <Input id="cur" type="password" autoComplete="current-password" value={current} onChange={(e) => setCurrent(e.target.value)} className="h-9 bg-card" />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="new">New password</Label>
          <Input id="new" type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} className="h-9 bg-card" />
          <p className="text-[13px] text-muted-foreground">At least 8 characters.</p>
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="again">New password again</Label>
          <Input id="again" type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} className="h-9 bg-card" aria-invalid={mismatch} />
          {mismatch && <p className="text-[13px] text-destructive">The two new passwords differ.</p>}
        </div>
        <Button type="submit" disabled={busy || !current || next.length < 8 || next !== again}>
          {busy ? 'Changing…' : 'Change password'}
        </Button>
      </form>
      <p className="mt-8 text-sm text-muted-foreground">adminui {s.data?.version}</p>
    </article>
  )
}
