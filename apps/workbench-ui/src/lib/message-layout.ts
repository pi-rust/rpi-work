// Local display preference only; never affects Agent admission or execution.
import { useCallback, useState } from 'react'

export type MessageLayout = 'reading' | 'classic'
const KEY = 'rpi-workbench-message-layout-v1'

function read(): MessageLayout {
  try { return localStorage.getItem(KEY) === 'classic' ? 'classic' : 'reading' }
  catch { return 'reading' }
}

export function useMessageLayout() {
  const [layout, update] = useState<MessageLayout>(read)
  const setLayout = useCallback((next: MessageLayout) => {
    update(next)
    try { localStorage.setItem(KEY, next) } catch { /* Keep the in-memory choice. */ }
  }, [])
  return { layout, setLayout }
}
