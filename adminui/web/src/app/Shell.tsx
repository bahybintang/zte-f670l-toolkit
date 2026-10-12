import { useMemo, useState, type ReactNode } from 'react'
import { Link, useRouterState } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { HardDrive, History, LayoutDashboard, LogOut, Menu, Monitor, Moon, Search, Sun, Table2, Users, Wrench, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { api } from '@/lib/api'
import { useSchema, useSession } from '@/lib/queries'
import { GROUPS, type Page } from '@/lib/schema'
import { getTheme, setTheme, type Theme } from '@/lib/theme'
import { GROUP_ICON } from '@/components/icons'
import { LedStrip } from '@/components/LedStrip'
import { ConfirmBanner } from '@/components/ConfirmBanner'
import { RestartBanner } from '@/components/RestartBanner'
import { useEffect } from 'react'
import { pending } from '@/lib/pending'
import { Login } from '@/pages/Login'
import { Button } from '@/components/ui/button'

export function Shell({ children }: { children: ReactNode }) {
  const session = useSession()
  if (session.isPending) return <Splash />
  if (!session.data?.authenticated) return <Login />
  return (
    <Authed defaultPassword={!!session.data.defaultPassword} restartPending={session.data.restartPending ?? []}>
      {children}
    </Authed>
  )
}

function Splash() {
  return (
    <div className="grid min-h-dvh place-items-center">
      <div className="flex items-center gap-2 text-muted-foreground">
        <span className="led" data-state="blink" /> Connecting to the router
      </div>
    </div>
  )
}

const TOOLS = [
  { to: '/', label: 'Overview', icon: LayoutDashboard },
  { to: '/clients', label: 'Devices', icon: Users },
  { to: '/tools', label: 'Network tools', icon: Wrench },
  { to: '/history', label: 'Change history', icon: History },
  { to: '/db', label: 'Config database', icon: Table2 },
  { to: '/flash', label: 'Flash and firmware', icon: HardDrive },
] as const

function Authed({ children, defaultPassword, restartPending }: { children: ReactNode; defaultPassword: boolean; restartPending: string[] }) {
  const [open, setOpen] = useState(false)
  // Re-open keep-or-revert countdowns after a reload or a reboot.
  useEffect(() => {
    api.get<{ pending: { token: string; entry: number; title: string; secondsLeft: number }[] }>('/pending')
      .then((r) => r.pending.forEach((p) => pending.add({ token: p.token, entry: p.entry, title: p.title, deadline: Date.now() + p.secondsLeft * 1000 })))
      .catch(() => {})
  }, [])
  const path = useRouterState({ select: (s) => s.location.pathname })
  return (
    <div className="min-h-dvh lg:grid lg:grid-cols-[264px_1fr]">
      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-40 w-[280px] border-r border-border bg-sidebar transition-transform duration-300 ease-out-expo lg:sticky lg:top-0 lg:h-dvh lg:w-auto lg:translate-x-0',
          open ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <Sidebar onNavigate={() => setOpen(false)} />
      </aside>
      {open && <div className="fixed inset-0 z-30 bg-foreground/20 lg:hidden" onClick={() => setOpen(false)} />}
      <div className="min-w-0">
        <header className="sticky top-0 z-20 flex h-14 items-center gap-3 border-b border-border bg-background/85 px-4 backdrop-blur sm:px-6">
          <Button variant="ghost" size="icon" className="lg:hidden" aria-label="Open menu" onClick={() => setOpen(true)}>
            <Menu />
          </Button>
          <div className="min-w-0 flex-1">
            <LedStrip />
          </div>
          <ThemeToggle />
          <LogoutButton />
        </header>
        {defaultPassword && path !== '/account' && (
          <div className="border-b border-led-warn/40 bg-led-warn/12 px-4 py-2 text-sm sm:px-6">
            You are still using the default password.{' '}
            <Link to="/account" className="font-medium text-link underline underline-offset-2">
              Change it now
            </Link>
          </div>
        )}
        <RestartBanner titles={restartPending} />
        <ConfirmBanner />
        <main className="mx-auto w-full max-w-[1120px] px-4 pt-6 pb-32 sm:px-8 sm:pt-9">{children}</main>
      </div>
    </div>
  )
}

function Sidebar({ onNavigate }: { onNavigate: () => void }) {
  const { data } = useSchema()
  const [q, setQ] = useState('')
  const path = useRouterState({ select: (s) => s.location.pathname })
  const groups = useMemo(() => {
    const pages = (data?.pages ?? []).filter((p) => p.applicable !== false && !p.id.startsWith('seed-'))
    const needle = q.trim().toLowerCase()
    const match = (p: Page) =>
      !needle || p.title.toLowerCase().includes(needle) || (p.description ?? '').toLowerCase().includes(needle)
    return GROUPS.map((g) => ({
      ...g,
      pages: pages
        .filter((p) => p.group === g.id && match(p))
        .sort((a, b) => (a.order ?? 100) - (b.order ?? 100) || a.title.localeCompare(b.title)),
    })).filter((g) => g.pages.length)
  }, [data, q])

  return (
    <nav aria-label="Main" className="flex h-full flex-col">
      <div className="flex h-14 items-center justify-between gap-2 border-b border-border px-5">
        <Link to="/" onClick={onNavigate} className="flex items-baseline gap-2">
          <span className="font-heading text-[19px] font-semibold tracking-tight">F670L</span>
          <span className="text-[13px] text-muted-foreground">admin</span>
        </Link>
        <Button variant="ghost" size="icon-sm" className="lg:hidden" aria-label="Close menu" onClick={onNavigate}>
          <X />
        </Button>
      </div>
      <div className="px-3 pt-3">
        <label className="flex h-9 items-center gap-2 rounded-md border border-input bg-card px-2.5 text-sm focus-within:outline-2 focus-within:outline-ring">
          <Search className="size-4 text-muted-foreground" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Find a setting"
            className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-muted-foreground"
          />
        </label>
      </div>
      <div className="flex-1 overflow-y-auto px-3 pt-3 pb-6 [scrollbar-width:thin]">
        <ul className="mb-4 space-y-px">
          {TOOLS.map((t) => (
            <li key={t.to}>
              <NavItem to={t.to} active={path === t.to} onClick={onNavigate} icon={<t.icon className="size-4" />}>
                {t.label}
              </NavItem>
            </li>
          ))}
        </ul>
        {groups.map((g) => {
          const Icon = GROUP_ICON[g.id]
          return (
            <section key={g.id} className="mb-4">
              <h2 className="flex items-center gap-2 px-2.5 pb-1 font-heading text-[13px] font-semibold text-muted-foreground">
                {Icon && <Icon className="size-3.5" />}
                {g.label}
              </h2>
              <ul className="space-y-px">
                {g.pages.map((p) => (
                  <li key={p.id}>
                    <NavItem
                      to={`/p/${p.id}`}
                      active={path === `/p/${p.id}`}
                      onClick={onNavigate}
                      muted={p.applicable === 'unknown'}
                    >
                      {p.title}
                    </NavItem>
                  </li>
                ))}
              </ul>
            </section>
          )
        })}
        {!groups.length && q && <p className="px-2.5 text-sm text-muted-foreground">No setting matches “{q}”.</p>}
      </div>
    </nav>
  )
}

function NavItem({
  to, active, children, icon, onClick, muted,
}: { to: string; active: boolean; children: ReactNode; icon?: ReactNode; onClick: () => void; muted?: boolean }) {
  return (
    <Link
      to={to}
      onClick={onClick}
      aria-current={active ? 'page' : undefined}
      className={cn(
        'relative flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[14px] transition-colors hover:bg-sidebar-accent',
        active && 'bg-card font-medium shadow-[0_1px_0_var(--border),0_0_0_1px_var(--border)]',
        muted && !active && 'text-muted-foreground',
      )}
    >
      {active && <span aria-hidden className="absolute top-1.5 bottom-1.5 left-0 w-[3px] rounded-full bg-foreground" />}
      {icon}
      <span className="truncate">{children}</span>
    </Link>
  )
}

function ThemeToggle() {
  const [t, setT] = useState<Theme>(getTheme())
  const next: Record<Theme, Theme> = { system: 'light', light: 'dark', dark: 'system' }
  const Icon = t === 'light' ? Sun : t === 'dark' ? Moon : Monitor
  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label={`Theme: ${t}. Switch to ${next[t]}`}
      title={`Theme: ${t}`}
      onClick={() => {
        setTheme(next[t])
        setT(next[t])
      }}
    >
      <Icon />
    </Button>
  )
}

function LogoutButton() {
  const qc = useQueryClient()
  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label="Sign out"
      title="Sign out"
      onClick={async () => {
        await api.post('/logout').catch(() => {})
        qc.clear()
        qc.setQueryData(['session'], { authenticated: false })
      }}
    >
      <LogOut />
    </Button>
  )
}
