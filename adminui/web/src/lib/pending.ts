import { useSyncExternalStore } from 'react'

// Commit-confirm changes awaiting "Keep": the server reverts them when the
// deadline passes, so the countdown must survive page navigation.
export interface PendingConfirm {
  token: string
  entry: number
  title: string
  deadline: number // ms epoch, client clock
}

let items: PendingConfirm[] = []
const subs = new Set<() => void>()

function emit() {
  subs.forEach((f) => f())
}

export const pending = {
  add(p: PendingConfirm) {
    items = [...items.filter((i) => i.token !== p.token), p]
    emit()
  },
  remove(token: string) {
    items = items.filter((i) => i.token !== token)
    emit()
  },
}

export function usePending() {
  return useSyncExternalStore(
    (f) => {
      subs.add(f)
      return () => subs.delete(f)
    },
    () => items,
  )
}
