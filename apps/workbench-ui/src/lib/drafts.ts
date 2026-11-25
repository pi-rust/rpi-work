/**
 * Per-scope draft storage.
 *
 * Drafts are local to this device and keyed by `origin/session`, so switching
 * sessions or nodes never leaks text into another conversation.
 */
import { useCallback, useRef, useState } from 'react'
import type { DraftFeedback } from '@/lib/viewmodel'
import { useScopedPresentation } from '@/lib/use-scoped-presentation'

const KEY = 'rpi-work-reui-drafts-v1'
const MAX_DRAFT = 32_000

const MAX_FEEDBACK = 32

export type FeedbackItem = DraftFeedback & { id: string }
// Track input and each feedback separately so an ACK can remove just what it sent.
type Draft = { input: { text: string }; feedback: FeedbackItem[] }
type Drafts = Record<string, Draft>
const emptyDraft: Draft = { input: { text: '' }, feedback: [] }
export type DraftSelection = { text: string; start: number; end: number; direction: 'forward' | 'backward' | 'none' }

function isFeedback(value: unknown): value is FeedbackItem {
  if (typeof value !== 'object' || value === null) return false
  const item = value as Partial<FeedbackItem>
  return typeof item.id === 'string' && typeof item.path === 'string' && typeof item.staged === 'boolean' &&
    typeof item.index === 'number' && Number.isSafeInteger(item.index) && item.index >= 0 &&
    typeof item.revision === 'string' && /^[0-9a-f]{64}$/.test(item.revision) && typeof item.location === 'string' &&
    typeof item.comment === 'string' && item.comment.length <= 2000 && typeof item.message === 'string' &&
    item.message.length > 0 && item.message.length <= MAX_DRAFT
}

function read(): Drafts {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? '{}') as unknown
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return {}
    return Object.fromEntries(Object.entries(value).flatMap(([scope, stored]) => {
      if (typeof stored === 'string') return [[scope, { input: { text: stored.slice(0, MAX_DRAFT) }, feedback: [] }]]
      if (!stored || typeof stored.text !== 'string' || !Array.isArray(stored.feedback)) return []
      const feedback = stored.feedback.filter(isFeedback).slice(0, MAX_FEEDBACK)
      return [[scope, { input: { text: stored.text.slice(0, MAX_DRAFT) }, feedback }]]
    }))
  } catch {
    return {}
  }
}

function write(drafts: Drafts) {
  const stored = Object.fromEntries(Object.entries(drafts)
    .filter(([, entry]) => entry.input.text.trim() || entry.feedback.length)
    .map(([scope, entry]) => {
      const text = entry.input.text.slice(0, MAX_DRAFT)
      return [scope, entry.feedback.length ? { text, feedback: entry.feedback } : text]
    }))
  try {
    localStorage.setItem(KEY, JSON.stringify(stored))
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
  const selection = useScopedPresentation<DraftSelection>(scope, () => ({ text: '', start: 0, end: 0, direction: 'none' }))
  const update = useCallback((entry: Draft) => {
    const next = { ...current.current, [scope]: entry }
    current.current = next
    setDrafts(next)
    write(next)
  }, [scope])

  const setDraft = (text: string) => update({ ...(current.current[scope] ?? emptyDraft), input: { text } })
  const addFeedback = (feedback: DraftFeedback) => {
    const entry = current.current[scope] ?? emptyDraft
    const size = entry.feedback.reduce((sum, item) => sum + 2 + item.message.length, entry.input.text.length)
    if (entry.feedback.length >= MAX_FEEDBACK || size + feedback.message.length + 2 > MAX_DRAFT) return false
    const id = crypto.getRandomValues(new Uint32Array(4)).join('-')
    update({ ...entry, feedback: [...entry.feedback, { ...feedback, id }] })
    return true
  }
  const removeFeedback = (id: string) => {
    const entry = current.current[scope] ?? emptyDraft
    update({ ...entry, feedback: entry.feedback.filter(item => item.id !== id) })
  }

  const entry = drafts[scope] ?? emptyDraft
  const sendText = [entry.input.text, ...entry.feedback.map(item => item.message)].filter(Boolean).join('\n\n')
  const clear = useCallback(() => {
    const latest = current.current[scope]
    if (!latest) return
    const input = latest.input === entry.input ? { text: '' } : latest.input
    const feedback = latest.feedback.filter(item => !entry.feedback.includes(item))
    if (input !== latest.input || feedback.length !== latest.feedback.length) update({ input, feedback })
  }, [scope, entry, update])

  const feedbackOverLimit = entry.feedback.length > 0 && sendText.length > MAX_DRAFT
  return { draft: entry.input.text, feedback: entry.feedback, sendText, feedbackOverLimit, setDraft, addFeedback, removeFeedback, clear, selection }
}
