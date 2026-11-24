import { useState } from 'react'

/** Per-App bounded display memory. New scopes are selected in the same render,
 * without exposing a previous scope or reading refs during rendering. */
export function useScopedPresentation<T extends object>(scope: string, create: () => T): T {
  const [entries, setEntries] = useState(() => new Map([[scope, create()]]))
  let value = entries.get(scope)
  if (!value) {
    value = create()
    const next = new Map(entries)
    next.set(scope, value)
    if (next.size > 128) next.delete(next.keys().next().value!)
    setEntries(next)
  }
  return value
}
