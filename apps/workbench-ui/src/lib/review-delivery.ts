import { useState } from 'react'

export type ReviewDelivery = 'inline' | 'detached'
const KEY = 'rpi-workbench-review-delivery'

export function useReviewDelivery() {
  const [delivery, setDelivery] = useState<ReviewDelivery>(() => {
    try { return localStorage.getItem(KEY) === 'detached' ? 'detached' : 'inline' }
    catch { return 'inline' }
  })
  const updateDelivery = (next: ReviewDelivery) => {
    setDelivery(next)
    try { localStorage.setItem(KEY, next) } catch { /* Keep the choice for the current page. */ }
  }
  return { delivery, updateDelivery }
}
