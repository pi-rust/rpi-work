import { useState } from 'react'

export type ReplyRating = 'helpful' | 'unhelpful'
const MAX_RATINGS = 512

function readRatings(key: string | null): Map<string, ReplyRating> {
  if (!key) return new Map()
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? '{}')
    if (!saved || typeof saved !== 'object' || Array.isArray(saved)) return new Map()
    return new Map(Object.entries(saved).filter((entry): entry is [string, ReplyRating] =>
      entry[1] === 'helpful' || entry[1] === 'unhelpful').slice(-MAX_RATINGS))
  } catch {
    return new Map()
  }
}

/** The conversation is keyed by node, project and session. */
export function useReplyRatings(scope?: string) {
  const key = scope ? `rpi-workbench-reply-ratings/${scope}` : null
  const [ratings, setRatings] = useState(() => readRatings(key))
  const toggleRating = (id: string, rating: ReplyRating) => {
    if (!key) return
    const next = new Map(ratings)
    next.delete(id)
    if (ratings.get(id) !== rating) next.set(id, rating)
    if (next.size > MAX_RATINGS) next.delete(next.keys().next().value!)
    setRatings(next)
    try { localStorage.setItem(key, JSON.stringify(Object.fromEntries(next))) }
    catch { /* The conversation still keeps the choice in memory. */ }
  }
  return { ratings, toggleRating }
}
