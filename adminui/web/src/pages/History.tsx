import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { Download } from 'lucide-react'
import { api } from '@/lib/api'
import { when } from '@/lib/format'
import { Button } from '@/components/ui/button'

interface Change { table: string; row: number; dm: string; old: string; new: string; secret: boolean }
interface Entry {
  id: number; ts: number; page: string; title: string; changes: Change[]; structural: boolean
  snapshot?: string; undone: boolean; undoOf?: number; pendingConfirm: boolean
}

export function History() {
  const qc = useQueryClient()
  const q = useQuery({ queryKey: ['history'], queryFn: async () => (await api.get<{ entries: Entry[] }>('/history')).entries })
  const undo = async (e: Entry) => {
    try {
      await api.post(`/history/${e.id}/undo`)
      toast.success(`Undid “${e.title}”.`)
      qc.invalidateQueries({ queryKey: ['history'] })
      qc.invalidateQueries({ queryKey: ['page'] })
    } catch (err) {
      toast.error((err as Error).message)
    }
  }
  return (
    <article>
      <header className="mb-7 flex flex-wrap items-end justify-between gap-4">
        <div className="max-w-2xl">
          <h1 className="font-heading text-[32px] font-semibold tracking-tight">Change history</h1>
          <p className="mt-1.5 text-muted-foreground">
            Every change made here, newest first. A configuration backup is kept for the latest five.
          </p>
        </div>
        <Button variant="outline" render={<a href="/api/backup/download" />}>
          <Download /> Download current configuration
        </Button>
      </header>
      {q.data?.length === 0 && <p className="text-muted-foreground">No changes yet. Settings you apply appear here, and you can undo them.</p>}
      <ol className="space-y-3">
        {q.data?.map((e) => (
          <li key={e.id} className="rounded-lg border border-border bg-card px-4 py-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <span className="font-medium">{e.title}</span>
                <span className="ml-3 text-[13px] text-muted-foreground">{when(e.ts)}</span>
                {e.undone && <span className="ml-3 rounded bg-muted px-1.5 py-0.5 text-[12px] text-muted-foreground">undone</span>}
                {e.pendingConfirm && <span className="ml-3 rounded bg-fiber/25 px-1.5 py-0.5 text-[12px]">waiting to be kept</span>}
              </div>
              {!e.undone && !e.structural && e.changes.length > 0 && (
                <Button variant="outline" size="sm" onClick={() => undo(e)}>Undo</Button>
              )}
            </div>
            {e.changes.length > 0 && (
              <ul className="num mt-2 space-y-0.5 text-[13px] text-muted-foreground">
                {e.changes.slice(0, 8).map((c, i) => (
                  <li key={i}>
                    {c.table}[{c.row}].{c.dm}: {c.secret ? 'password changed' : <><s>{c.old || '∅'}</s> → <span className="text-foreground">{c.new || '∅'}</span></>}
                  </li>
                ))}
                {e.changes.length > 8 && <li>and {e.changes.length - 8} more</li>}
              </ul>
            )}
            {e.structural && <p className="mt-1 text-[13px] text-muted-foreground">Rows were added or removed. Undo by restoring its backup.</p>}
          </li>
        ))}
      </ol>
    </article>
  )
}
