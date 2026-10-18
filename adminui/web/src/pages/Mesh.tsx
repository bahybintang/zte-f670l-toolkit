import { useEffect, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { MeshMap, type MeshNode } from '@/components/MeshMap'

type Role = 'controller' | 'agent'
type Backhaul = 'ethernet' | 'wifi'

interface Settings {
  enabled: boolean
  role: Role
  backhaul: Backhaul
  ssid: string
  passwordSet?: boolean
  bandSteering: boolean
  roamRssi24: number
  roamRssi5: number
  steerRssi: number
  uplinkSsid: string
  uplinkPasswordSet?: boolean
}

interface Status {
  running: boolean
  daemons: Record<string, boolean>
  nodes?: MeshNode[]
  topology: string
  agent?: { controller: string | null; configured: boolean } | null
  wifiBackhaul?: { station: string; band: string; ap: string } | null
  uplink?: 'off' | 'connecting' | 'cabled' | 'bridged'
}

interface MeshResp {
  settings: Settings
  status: Status
  message?: string
}

/** EasyMesh, modelled on the ZTE V9 "NetSphere" page: one network across every node. */
export function Mesh() {
  const qc = useQueryClient()
  const q = useQuery({
    queryKey: ['mesh'],
    queryFn: () => api.get<MeshResp>('/mesh'),
  })
  const st = useQuery({
    queryKey: ['mesh-status'],
    queryFn: () => api.get<{ status: Status }>('/mesh/status'),
    refetchInterval: 10_000,
  })
  const [s, setS] = useState<Settings | null>(null)
  const [password, setPassword] = useState('')
  const [uplinkPassword, setUplinkPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<string | null>(null)

  useEffect(() => {
    if (q.data && !s) setS(q.data.settings)
  }, [q.data, s])

  if (q.isPending || !s) return <Loading />
  if (q.isError) return <p className="text-sm text-destructive">{(q.error as Error).message}</p>

  const status = st.data?.status ?? q.data?.status
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((p) => (p ? { ...p, [k]: v } : p))
  const needsPassword = s.enabled && s.role === 'controller' && !s.passwordSet && password.length === 0
  const wifiAgent = s.role === 'agent' && s.backhaul === 'wifi'
  const savedUplink = q.data?.settings.uplinkSsid ?? ''
  const needsUplinkPassword =
    s.enabled &&
    wifiAgent &&
    s.uplinkSsid.length > 0 &&
    uplinkPassword.length === 0 &&
    !(s.uplinkPasswordSet && s.uplinkSsid === savedUplink)

  const save = async () => {
    setBusy(true)
    setResult(null)
    try {
      const r = await api.post<MeshResp>('/mesh', {
        ...s,
        password,
        uplinkPassword,
      })
      setS(r.settings)
      setPassword('')
      setUplinkPassword('')
      qc.setQueryData(['mesh'], r)
      setResult(r.message ?? 'Saved.')
      toast.success(r.message ?? 'Saved.')
      qc.setQueryData(['mesh-status'], { status: r.status })
    } catch (e) {
      const m = (e as Error).message
      setResult(m)
      toast.error(m)
    } finally {
      setBusy(false)
    }
  }

  const pair = async () => {
    try {
      const r = await api.post<{ message: string }>('/mesh/pair', {})
      setResult(r.message)
      toast.success(r.message)
    } catch (e) {
      const m = (e as Error).message
      setResult(m)
      toast.error(m)
    }
  }

  return (
    <article className="max-w-3xl">
      <h1 className="font-heading text-[32px] font-semibold tracking-tight">EasyMesh</h1>
      <p className="mt-1.5 mb-6 text-muted-foreground">
        One Wi-Fi network across this router and other EasyMesh devices. As the controller, this router hands its
        network name and password to every device that joins, and steers phones and laptops to the best one.
      </p>

      <StatusCard status={status} enabled={s.enabled} role={s.role} />

      <div className="mt-8 space-y-6">
        <Row label="EasyMesh" help="Starts the mesh service on this router.">
          <Switch checked={s.enabled} onCheckedChange={(v) => set('enabled', v)} aria-label="EasyMesh" />
        </Row>

        <Row
          label="Role"
          help="Controller on the main router. Agent joins another EasyMesh router (for example a TP-Link Archer or Deco): connect one of this router's LAN ports to it with a cable; no WPS needed."
        >
          <Segmented<Role>
            value={s.role}
            onChange={(v) => set('role', v)}
            options={[
              ['controller', 'Controller'],
              ['agent', 'Agent'],
            ]}
          />
        </Row>

        <Row label="Connection between devices" help="Ethernet when the devices are cabled together; Wi-Fi otherwise.">
          <Segmented<Backhaul>
            value={s.backhaul}
            onChange={(v) => set('backhaul', v)}
            options={[
              ['ethernet', 'Ethernet'],
              ['wifi', 'Wi-Fi'],
            ]}
          />
        </Row>

        {s.role === 'controller' && (
          <>
            <Row label="Mesh Wi-Fi name" help="Broadcast by every device in the mesh. No spaces, up to 29 characters.">
              <Input value={s.ssid} onChange={(e) => set('ssid', e.target.value)} className="h-9 max-w-sm bg-card" />
            </Row>
            <Row
              label="Mesh Wi-Fi password"
              help={s.passwordSet ? 'Saved. Leave empty to keep it.' : '8–63 characters, no spaces.'}
            >
              <Input
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={s.passwordSet ? '••••••••' : ''}
                className="h-9 max-w-sm bg-card"
              />
            </Row>
          </>
        )}

        {wifiAgent && (
          <>
            <Row
              label="Main router's Wi-Fi"
              help="The main router's 5 GHz network name. This router joins it directly; leave empty to pair with WPS instead."
            >
              <Input
                value={s.uplinkSsid}
                onChange={(e) => set('uplinkSsid', e.target.value)}
                autoComplete="off"
                className="h-9 max-w-sm bg-card"
              />
            </Row>
            {s.uplinkSsid.length > 0 && (
              <Row
                label="Main router's Wi-Fi password"
                help={
                  s.uplinkPasswordSet && s.uplinkSsid === savedUplink
                    ? 'Saved. Leave empty to keep it.'
                    : 'Stored on this router only; never shown again.'
                }
              >
                <Input
                  type="password"
                  autoComplete="new-password"
                  value={uplinkPassword}
                  onChange={(e) => setUplinkPassword(e.target.value)}
                  placeholder={s.uplinkPasswordSet && s.uplinkSsid === savedUplink ? '••••••••' : ''}
                  className="h-9 max-w-sm bg-card"
                />
              </Row>
            )}
            <p className="text-[13px] text-muted-foreground sm:pl-[236px]">
              Unplug the cable between the two routers. While a cable connects them, this router keeps the Wi-Fi link
              out of its network so the two paths can't form a loop.
            </p>
          </>
        )}

        <Row label="Band steering" help="Moves capable devices between 2.4 GHz and 5 GHz, and between mesh devices.">
          <Switch checked={s.bandSteering} onCheckedChange={(v) => set('bandSteering', v)} aria-label="Band steering" />
        </Row>

        <Row
          label="Roaming threshold"
          help="Below this signal a device is moved to a better mesh device (dBm, −95 to −30)."
        >
          <div className="flex flex-wrap gap-4">
            <Dbm label="2.4 GHz" value={s.roamRssi24} onChange={(v) => set('roamRssi24', v)} />
            <Dbm label="5 GHz" value={s.roamRssi5} onChange={(v) => set('roamRssi5', v)} />
          </div>
        </Row>

        <Row label="Steering threshold" help="Signal at which a mesh device hands a client to another one (dBm).">
          <Dbm value={s.steerRssi} onChange={(v) => set('steerRssi', v)} />
        </Row>
      </div>

      {s.enabled && wifiAgent && !s.uplinkSsid && q.data?.settings.enabled && (
        <div className="mt-8 rounded-lg border border-border bg-card p-4">
          <div className="text-sm font-medium">Pair over Wi-Fi</div>
          <p className="mt-1 text-[13px] text-muted-foreground">
            Start “Add satellite” on the main router (or press its WPS button), then press Pair. This router keeps
            trying for up to 10 minutes and connects to the main router’s hidden backhaul network.
          </p>
          <Button variant="outline" className="mt-3" onClick={pair} disabled={busy}>
            Pair
          </Button>
        </div>
      )}

      <div className="mt-8 flex items-center gap-3 border-t border-border pt-5">
        <Button onClick={save} disabled={busy || needsPassword || needsUplinkPassword}>
          {busy ? 'Applying…' : 'Apply'}
        </Button>
        {needsPassword && (
          <span className="text-[13px] text-muted-foreground">Set a mesh Wi-Fi password to turn it on.</span>
        )}
        {needsUplinkPassword && (
          <span className="text-[13px] text-muted-foreground">Enter the main router's Wi-Fi password.</span>
        )}
      </div>
      {result && (
        <p className="mt-4 rounded-lg border border-border bg-card px-4 py-3 text-sm text-muted-foreground">{result}</p>
      )}
    </article>
  )
}

function StatusCard({ status, enabled, role }: { status?: Status; enabled: boolean; role: Role }) {
  const up = status?.running
  const tone = up ? 'bg-emerald-500' : enabled ? 'bg-amber-500' : 'bg-muted-foreground/40'
  const text = up ? 'Running' : enabled ? 'Enabled, not running' : 'Off'
  return (
    <section className="rounded-xl border border-border bg-card p-4">
      <div className="flex items-center gap-2 text-sm font-medium">
        <span className={`size-2 rounded-full ${tone}`} aria-hidden /> {text}
        {status && (
          <span className="ml-auto text-[12px] font-normal text-muted-foreground">
            {Object.entries(status.daemons)
              .map(([d, ok]) => `${d} ${ok ? '✓' : '✗'}`)
              .join(' · ')}
          </span>
        )}
      </div>
      {up && status?.agent && (
        <p className="mt-3 text-[13px] text-muted-foreground">
          {status.agent.controller ? (
            <>
              Joined{' '}
              {(() => {
                const name = status.nodes?.find((n) => n.almac === status.agent?.controller)?.name
                return name ? (
                  <>
                    <span className="font-medium text-foreground">{name}</span>{' '}
                    <span className="font-mono">({status.agent.controller})</span>
                  </>
                ) : (
                  <span className="font-mono text-foreground">controller {status.agent.controller}</span>
                )
              })()}
              {status.agent.configured ? ' · Wi-Fi settings received from it' : ' · waiting for its Wi-Fi settings'}
            </>
          ) : (
            'Looking for an EasyMesh controller on the LAN…'
          )}
        </p>
      )}
      {up && status?.agent && (
        <p className="mt-1 text-[13px] text-muted-foreground">
          {status.wifiBackhaul ? (
            <>
              Wi-Fi backhaul: {status.wifiBackhaul.band} link to{' '}
              <span className="font-mono">{status.wifiBackhaul.ap}</span>
              {status.uplink === 'bridged' && ' · in use'}
              {status.uplink === 'cabled' && ' · not in use yet: a cable to the main router is (or was just) connected'}
            </>
          ) : status.uplink === 'connecting' ? (
            'Wi-Fi backhaul: connecting…'
          ) : (
            'Wi-Fi backhaul: not connected'
          )}
        </p>
      )}
      {up && (
        <div className="mt-4 rounded-lg bg-background/60 p-3">
          <MeshMap nodes={status?.nodes ?? []} role={role} />
          <details className="mt-3">
            <summary className="cursor-pointer text-[13px] text-muted-foreground">Details</summary>
            <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap rounded-md bg-background p-3 font-mono text-[12px]">
              {status?.topology || 'No topology reported yet.'}
            </pre>
          </details>
        </div>
      )}
    </section>
  )
}

function Row({ label, help, children }: { label: string; help?: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-2 sm:grid-cols-[220px_1fr] sm:items-start">
      <div>
        <div className="text-sm font-medium">{label}</div>
        {help && <p className="mt-0.5 text-[13px] text-muted-foreground">{help}</p>}
      </div>
      <div className="sm:pt-0.5">{children}</div>
    </div>
  )
}

function Segmented<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T
  onChange: (v: T) => void
  options: [T, string][]
}) {
  return (
    <div className="inline-flex rounded-md border border-input bg-card p-0.5" role="radiogroup">
      {options.map(([v, label]) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={value === v}
          onClick={() => onChange(v)}
          className={
            value === v
              ? 'rounded px-3 py-1 text-[14px] font-medium bg-foreground text-background'
              : 'rounded px-3 py-1 text-[14px] text-muted-foreground hover:text-foreground'
          }
        >
          {label}
        </button>
      ))}
    </div>
  )
}

function Dbm({ label, value, onChange }: { label?: string; value: number; onChange: (v: number) => void }) {
  return (
    <label className="flex items-center gap-2 text-[13px]">
      {label && <span className="text-muted-foreground">{label}</span>}
      <Input
        type="number"
        min={-95}
        max={-30}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="h-9 w-24 bg-card"
      />
      <span className="text-muted-foreground">dBm</span>
    </label>
  )
}

function Loading() {
  return (
    <div className="max-w-3xl space-y-3">
      <Skeleton className="h-8 w-48" />
      <Skeleton className="h-4 w-96 max-w-full" />
      <Skeleton className="mt-6 h-20 w-full" />
    </div>
  )
}
