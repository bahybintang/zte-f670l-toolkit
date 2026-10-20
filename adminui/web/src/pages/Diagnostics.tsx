import { useState, type FormEvent } from 'react'
import { api } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

const TOOLS = [
  { id: 'ping', label: 'Ping', hint: 'Checks that a host answers, and how fast.' },
  { id: 'traceroute', label: 'Traceroute', hint: 'Shows each router on the path to a host.' },
  { id: 'nslookup', label: 'DNS lookup', hint: 'Resolves a name using the router’s DNS.' },
] as const

export function Diagnostics() {
  const [tool, setTool] = useState<(typeof TOOLS)[number]['id']>('ping')
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
  const t = TOOLS.find((x) => x.id === tool)!
  return (
    <article>
      <header className="mb-6 max-w-2xl">
        <h1 className="font-heading text-[32px] font-semibold tracking-tight">Network tools</h1>
        <p className="mt-1.5 text-muted-foreground">Run tests from the router itself.</p>
      </header>
      <div className="mb-4 flex gap-1.5" role="tablist">
        {TOOLS.map((x) => (
          <button
            key={x.id}
            role="tab"
            aria-selected={tool === x.id}
            onClick={() => setTool(x.id)}
            className={
              tool === x.id
                ? 'rounded-md border border-foreground bg-foreground px-3 py-1.5 text-[14px] font-medium text-background'
                : 'rounded-md border border-input bg-card px-3 py-1.5 text-[14px] hover:border-foreground/40'
            }
          >
            {x.label}
          </button>
        ))}
      </div>
      <p className="mb-3 text-sm text-muted-foreground">{t.hint}</p>
      <form onSubmit={run} className="flex max-w-lg gap-2">
        <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="Host name or IP address" className="h-9 bg-card" />
        <Button type="submit" disabled={busy || !host.trim()}>{busy ? 'Running…' : `Run ${t.label.toLowerCase()}`}</Button>
      </form>
      {(out || busy) && (
        <pre className="num mt-5 max-h-[60dvh] overflow-auto rounded-lg border border-border bg-card p-4 text-[13px] leading-relaxed whitespace-pre-wrap">
          {busy ? 'Waiting for the router…' : out}
        </pre>
      )}
    </article>
  )
}
