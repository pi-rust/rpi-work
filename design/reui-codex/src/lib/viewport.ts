/** Match the confirmed shell: navigation drawer at 700px, preview overlay at 900px. */
import { useEffect, useState } from 'react'

export const MOBILE_QUERY = '(max-width: 700px)'
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => typeof window !== 'undefined' && window.matchMedia(query).matches)
  useEffect(() => {
    const media = window.matchMedia(query)
    const update = () => setMatches(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [query])
  return matches
}
export function useIsMobile(): boolean { return useMediaQuery(MOBILE_QUERY) }
