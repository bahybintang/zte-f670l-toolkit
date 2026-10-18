import { useState, type FormEvent } from 'react'
import { api } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/** Runs a `/diag/{tool}` command (ping/traceroute/nslookup) live on the router. */
export function DiagnosticRunner({ tool, label }: { tool: string; label: string }) {
  const [host, setHost] = useState('')
  const [out, setOut] = useState('')
  const [busy, setBusy] = useState(false)
  const run = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setOut('')
    try {
      const r = await api.post<{ output: string }>(`/diag/${tool}`, { host: host.trim() })
      setOut(r.output || '(no output)')
    } catch (err) {
      setOut((err as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <div>
      <form onSubmit={run} className="flex max-w-lg gap-2">
        <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="Host name or IP address" className="h-9 bg-card" autoFocus />
        <Button type="submit" disabled={busy || !host.trim()}>{busy ? 'Running…' : `Run ${label.toLowerCase()}`}</Button>
      </form>
      {(out || busy) && (
        <pre className="num mt-5 max-h-[60dvh] overflow-auto rounded-lg border border-border bg-card p-4 text-[13px] leading-relaxed whitespace-pre-wrap">
          {busy ? 'Waiting for the router…' : out}
        </pre>
      )}
    </div>
  )
}
