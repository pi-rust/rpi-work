/**
 * Per-scope draft storage.
 *
 * Drafts are local to this device and keyed by `origin/session`, so switching
 * sessions or nodes never leaks text into another conversation.
 */
import { useCallback, useRef, useState } from 'react'

const KEY = 'rpi-work-reui-drafts-v1'
const MAX_DRAFT = 32_000

type Drafts = Record<string, string>

function read(): Drafts {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return {}
    const value = JSON.parse(raw) as unknown
    if (typeof value !== 'object' || value === null) return {}
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([, text]) => typeof text === 'string')
        .map(([scope, text]) => [scope, (text as string).slice(0, MAX_DRAFT)]),
    )
  } catch {
    return {}
  }
}

function write(drafts: Drafts) {
  try {
    localStorage.setItem(KEY, JSON.stringify(drafts))
  } catch {
    // A full or blocked quota must not break typing; the draft stays in memory.
  }
}

export function useDraft(scope: string) {
  // Derive from the scope in the same render, rather than showing the previous
  // scope's text until an effect runs. The ref also keeps in-memory drafts when
  // storage is blocked, and makes consecutive edits merge without stale state.
  const [drafts, setDrafts] = useState<Drafts>(read)
  const current = useRef(drafts)

  const update = useCallback(
    (value: string) => {
      const next = { ...current.current, [scope]: value }
      current.current = next
      setDrafts(next)
      write(Object.fromEntries(Object.entries(next)
        .filter(([, text]) => text.trim().length > 0)
        .map(([key, text]) => [key, text.slice(0, MAX_DRAFT)])))
    },
    [scope],
  )

  const clear = useCallback(() => update(''), [update])

  return { draft: drafts[scope] ?? '', setDraft: update, clear }
}
