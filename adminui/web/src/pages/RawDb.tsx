import { useMemo, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Notice } from './SchemaPage'

interface RawRow { no: number; fields: [string, string, boolean][] }

/** Every table in the config DB, for anything no page covers yet. */
export function RawDb() {
  const tables = useQuery({ queryKey: ['tables'], queryFn: async () => (await api.get<{ tables: string[] }>('/db/tables')).tables })
  const [q, setQ] = useState('')
  const [sel, setSel] = useState<string | null>(null)
  const list = useMemo(() => (tables.data ?? []).filter((t) => t.toLowerCase().includes(q.toLowerCase())), [tables.data, q])
  return (
    <article>
      <header className="mb-6 max-w-2xl">
        <h1 className="font-heading text-[32px] font-semibold tracking-tight">Config database</h1>
        <p className="mt-1.5 text-muted-foreground">
          The router’s raw settings tables. Each edit is verified, saved and recorded in the change history.
        </p>
      </header>
      <Notice tone="warn">Values here are not checked against what the firmware expects. A wrong value can break a feature until you undo it.</Notice>
      <div className="grid gap-6 lg:grid-cols-[240px_1fr]">
        <div>
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Filter ${tables.data?.length ?? ''} tables`} className="mb-2 h-9 bg-card" />
          <ul className="max-h-[65dvh] overflow-y-auto rounded-lg border border-border bg-card py-1 text-[14px]">
            {list.map((t) => (
              <li key={t}>
                <button
                  onClick={() => setSel(t)}
                  className={cn('num w-full px-3 py-1 text-left hover:bg-muted', sel === t && 'bg-muted font-medium')}
                >
                  {t}
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div className="min-w-0">{sel ? <TableView key={sel} name={sel} /> : <p className="text-muted-foreground">Pick a table to see its rows.</p>}</div>
      </div>
    </article>
  )
}

function TableView({ name }: { name: string }) {
  const qc = useQueryClient()
  const t = useQuery({ queryKey: ['rawtable', name], queryFn: () => api.get<{ rows: RawRow[] }>(`/db/table/${name}`) })
  const [edit, setEdit] = useState<{ row: number; dm: string; value: string } | null>(null)
  const save = async () => {
    if (!edit) return
    try {
      await api.post('/db/set', { table: name, row: edit.row, dm: edit.dm, value: edit.value })
      toast.success(`Saved ${name}[${edit.row}].${edit.dm}`)
      setEdit(null)
      qc.invalidateQueries({ queryKey: ['rawtable', name] })
    } catch (e) {
      toast.error((e as Error).message)
    }
  }
  if (t.isPending) return <p className="text-muted-foreground">Reading {name}…</p>
  if (t.isError) return <p className="text-destructive">{(t.error as Error).message}</p>
  const rows = t.data.rows
  return (
    <div>
      <h2 className="num mb-3 font-heading text-[20px] font-semibold">
        {name} <span className="text-[14px] font-normal text-muted-foreground">{rows.length} {rows.length === 1 ? 'row' : 'rows'}</span>
      </h2>
      {rows.length === 0 && <p className="text-muted-foreground">This table is empty.</p>}
      <div className="space-y-5">
        {rows.map((r) => (
          <div key={r.no} className="overflow-hidden rounded-lg border border-border bg-card">
            <div className="border-b border-border bg-muted/50 px-3 py-1.5 text-[13px] font-medium">Row {r.no}</div>
            <table className="w-full text-[13.5px]">
              <tbody className="divide-y divide-border/60">
                {r.fields.map(([dm, v, secret]) => {
                  const editing = edit?.row === r.no && edit.dm === dm
                  return (
                    <tr key={dm}>
                      <td className="num w-[38%] px-3 py-1 text-muted-foreground">{dm}</td>
                      <td className="num px-3 py-1">
                        {editing ? (
                          <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); save() }}>
                            <Input autoFocus value={edit.value} onChange={(e) => setEdit({ ...edit, value: e.target.value })} className="h-8 bg-card" />
                            <Button size="sm" type="submit">Save</Button>
                            <Button size="sm" variant="ghost" type="button" onClick={() => setEdit(null)}>Cancel</Button>
                          </form>
                        ) : (
                          <button
                            className="w-full truncate text-left hover:text-link"
                            title="Edit"
                            onClick={() => setEdit({ row: r.no, dm, value: secret ? '' : v })}
                          >
                            {v === '' ? <span className="text-muted-foreground">∅</span> : v}
                          </button>
                        )}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        ))}
      </div>
    </div>
  )
}
