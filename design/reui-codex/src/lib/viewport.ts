/**
 * Viewport helpers.
 *
 * The prototype renders a single tree and switches presentation by width; the
 * breakpoint matches the `md` Tailwind step the layout already uses.
 */
import { useEffect, useState } from 'react'

export const MOBILE_QUERY = '(max-width: 767px)'

export function useIsMobile(): boolean {
  const [mobile, setMobile] = useState(() =>
    typeof window === 'undefined' ? false : window.matchMedia(MOBILE_QUERY).matches,
  )
  useEffect(() => {
    const query = window.matchMedia(MOBILE_QUERY)
    const update = () => setMobile(query.matches)
    update()
    query.addEventListener('change', update)
    return () => query.removeEventListener('change', update)
  }, [])
  return mobile
}
