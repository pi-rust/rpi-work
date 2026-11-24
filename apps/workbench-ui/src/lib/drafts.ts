/**
 * Per-scope draft storage.
 *
 * Drafts are local to this device and keyed by `origin/session`, so switching
 * sessions or nodes never leaks text into another conversation.
 */
import { useCallback, useRef, useState } from 'react'
import { useScopedPresentation } from '@/lib/use-scoped-presentation'

const KEY = 'rpi-work-reui-drafts-v1'
const MAX_DRAFT = 32_000

type Drafts = Record<string, string>
export type DraftSelection = { text: string; start: number; end: number; direction: 'forward' | 'backward' | 'none' }

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
  const [drafts, setDrafts] = useState<Record<string, { text: string }>>(() =>
    Object.fromEntries(Object.entries(read()).map(([key, text]) => [key, { text }])),
  )
  const current = useRef(drafts)
  const selection = useScopedPresentation<DraftSelection>(scope, () => ({ text: '', start: 0, end: 0, direction: 'none' }))

  const update = useCallback(
    (value: string) => {
      const next = { ...current.current, [scope]: { text: value } }
      current.current = next
      setDrafts(next)
      write(Object.fromEntries(Object.entries(next)
        .filter(([, entry]) => entry.text.trim().length > 0)
        .map(([key, entry]) => [key, entry.text.slice(0, MAX_DRAFT)])))
    },
    [scope],
  )

  const entry = drafts[scope]
  const clear = useCallback(() => {
    // Clear only the exact edit that was submitted. A later edit gets a new
    // entry object, even when its text eventually equals the submitted text.
    if (current.current[scope] === entry) update('')
  }, [scope, entry, update])

  return { draft: entry?.text ?? '', setDraft: update, clear, selection }
}
