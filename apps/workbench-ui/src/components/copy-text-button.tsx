import { useLayoutEffect, useRef, useState } from 'react'
import { Check, Copy, Loader2 } from 'lucide-react'

type State = 'pending' | 'success' | 'error'

/** Clipboard is a user-gesture display action, never an execution operation.
 * A late result may describe only the exact payload and mounted control that
 * requested it. No legacy copy fallback steals the draft's selection/focus. */
export function CopyTextButton({ text, label }: { text: string; label: string }) {
  const serial = useRef(0)
  const [feedback, setFeedback] = useState<{ text: string; state: State } | null>(null)
  const state = feedback?.text === text ? feedback.state : 'idle'
  // Same-render payload reset also prevents A → B → A from reviving feedback.
  if (feedback && feedback.text !== text) setFeedback(null)
  useLayoutEffect(() => () => { serial.current++ }, [text])
  const copy = async () => {
    const ticket = ++serial.current
    setFeedback({ text, state: 'pending' })
    try {
      if (!navigator.clipboard?.writeText) throw Error('Clipboard unavailable')
      await navigator.clipboard.writeText(text)
      if (serial.current === ticket) setFeedback({ text, state: 'success' })
    } catch {
      if (serial.current === ticket) setFeedback({ text, state: 'error' })
    }
  }
  const status = state === 'success' ? '已复制' : state === 'error' ? '复制失败，请选择文字后手动复制' : state === 'pending' ? '正在复制' : ''
  return <span className="wb-copy-control">
    <button type="button" aria-label={label} title={status || label} data-copy-state={state} disabled={state === 'pending'} onClick={() => void copy()}>
      {state === 'success' ? <Check aria-hidden="true" /> : state === 'pending' ? <Loader2 aria-hidden="true" /> : <Copy aria-hidden="true" />}
    </button>
    <span role="status" className={state === 'error' ? 'text-destructive' : 'text-muted-foreground'}>{status}</span>
  </span>
}
