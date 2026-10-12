import { useState, type FormEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { api, setCsrf } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

export function Login() {
  const qc = useQueryClient()
  const [user, setUser] = useState('admin')
  const [pass, setPass] = useState('')
  const [err, setErr] = useState('')
  const [busy, setBusy] = useState(false)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setErr('')
    try {
      const r = await api.post<{ csrf: string }>('/login', { user, pass })
      setCsrf(r.csrf)
      await qc.invalidateQueries({ queryKey: ['session'] })
    } catch (ex) {
      setErr((ex as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="grid min-h-dvh place-items-center px-4">
      <div className="w-full max-w-[380px]">
        <div className="mb-8 flex items-center gap-3">
          <div className="flex gap-1.5" aria-hidden>
            <span className="led" data-state="ok" />
            <span className="led" />
            <span className="led" data-state="ok" />
          </div>
          <h1 className="font-heading text-2xl font-semibold tracking-tight">F670L admin</h1>
        </div>
        <form onSubmit={submit} className="space-y-4 rounded-xl border border-border bg-card p-6 shadow-[0_1px_2px_rgb(22_32_46/0.06),0_12px_32px_-16px_rgb(22_32_46/0.18)]">
          <div className="space-y-1.5">
            <Label htmlFor="user">User name</Label>
            <Input id="user" autoComplete="username" value={user} onChange={(e) => setUser(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pass">Password</Label>
            <Input id="pass" type="password" autoComplete="current-password" autoFocus value={pass} onChange={(e) => setPass(e.target.value)} />
          </div>
          {err && (
            <p role="alert" className="text-sm text-destructive">
              {err}
            </p>
          )}
          <Button type="submit" size="lg" className="w-full" disabled={busy || !pass}>
            {busy ? 'Signing in…' : 'Sign in'}
          </Button>
        </form>
        <p className="mt-4 text-[13px] text-muted-foreground">
          Forgot it? Over telnet, run <code className="rounded bg-muted px-1">adminui passwd</code> in{' '}
          <code className="rounded bg-muted px-1">/opt/upt/apps/adminui</code>, then restart adminui.
        </p>
      </div>
    </div>
  )
}
