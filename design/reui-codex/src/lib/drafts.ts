/**
 * Per-scope draft storage.
 *
 * Drafts are local to this device and keyed by `origin/session`, so switching
 * sessions or nodes never leaks text into another conversation.
 */
import { useCallback, useEffect, useRef, useState } from 'react'

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
  const [draft, setDraft] = useState('')
  const drafts = useRef<Drafts>({})

  useEffect(() => {
    drafts.current = read()
    setDraft(drafts.current[scope] ?? '')
  }, [scope])

  const update = useCallback(
    (value: string) => {
      setDraft(value)
      const next = { ...drafts.current }
      if (value.trim().length === 0) delete next[scope]
      else next[scope] = value.slice(0, MAX_DRAFT)
      drafts.current = next
      write(next)
    },
    [scope],
  )

  const clear = useCallback(() => update(''), [update])

  return { draft, setDraft: update, clear }
}
