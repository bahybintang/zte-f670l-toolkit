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

  const PANEL = ['PON', 'LOS', 'Net', 'LAN', '2.4G', '5G', 'USB']
  return (
    <div className="relative grid min-h-dvh place-items-center overflow-hidden px-4">
      {/* Faint equipment-rack grid + a soft glow, so the page reads as a device, not a void. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 opacity-[0.5]"
        style={{
          backgroundImage:
            'radial-gradient(60rem 40rem at 50% -10%, color-mix(in oklab, var(--ring) 10%, transparent), transparent 70%), linear-gradient(var(--border) 1px, transparent 1px), linear-gradient(90deg, var(--border) 1px, transparent 1px)',
          backgroundSize: '100% 100%, 44px 44px, 44px 44px',
          maskImage: 'radial-gradient(70% 60% at 50% 40%, black, transparent)',
        }}
      />
      <div className="relative w-full max-w-[380px]">
        <div className="mb-7">
          <div className="flex items-center gap-2.5">
            <span className="font-heading text-[26px] font-semibold tracking-tight">F670L</span>
            <span className="text-[15px] text-muted-foreground">admin</span>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1.5" aria-hidden>
            {PANEL.map((l, i) => (
              <span key={l} className="flex items-center gap-1.5 text-[11px] uppercase tracking-wide text-muted-foreground">
                <span className="led" data-state={i === 1 ? 'err' : i < 5 ? 'ok' : undefined} />
                {l}
              </span>
            ))}
          </div>
        </div>
        <form onSubmit={submit} className="space-y-4 rounded-xl border border-border bg-card/90 p-6 shadow-[0_1px_2px_rgb(22_32_46/0.06),0_18px_48px_-20px_rgb(22_32_46/0.30)] backdrop-blur">
          <p className="text-sm text-muted-foreground">Sign in to manage this router.</p>
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
