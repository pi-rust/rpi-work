import { useCallback, useLayoutEffect, useRef, useState } from 'react'
import type { ConversationPart } from '@/lib/viewmodel'
import { createReadingCheckpoints, type ReadingCheckpoint } from '@/lib/reading-checkpoints'

type Anchor = { key: string; top: number }
type Position = ReadingCheckpoint & { readingAnchor: Anchor | null; unread: boolean }
const BOTTOM_TOLERANCE = 48
const initialPosition = (saved?: ReadingCheckpoint): Position => ({
  top: saved?.top ?? 0, atBottom: saved?.atBottom ?? true, anchor: null, readingAnchor: saved?.anchor ?? null, unread: false,
})

function measureAnchors(container: HTMLElement): Pick<Position, 'anchor' | 'readingAnchor'> {
  const top = container.getBoundingClientRect().top
  for (const element of container.querySelectorAll<HTMLElement>('[data-conversation-anchor]')) {
    const rect = element.getBoundingClientRect()
    if (rect.bottom <= top + 1) continue
    const offset = rect.top - top
    return {
      anchor: { key: element.dataset.conversationAnchor!, top: offset },
      readingAnchor: element.dataset.readingAnchor ? { key: element.dataset.readingAnchor, top: offset } : null,
    }
  }
  return { anchor: null, readingAnchor: null }
}

function signature(parts: ConversationPart[]): string {
  return JSON.stringify(parts.map(part => part.kind === 'message'
    ? [part.key, part.text]
    : [part.key, ...part.items.map(item => item.kind === 'tool' ? [item.key, item.block.arguments, item.block.output, item.block.status] : [item.key, item.text])]))
}

function retain<K, V>(map: Map<K, V>, key: K, value: V) {
  map.delete(key)
  map.set(key, value)
  if (map.size > 128) map.delete(map.keys().next().value!)
}

/** Display-only memory per node/project/session; never an execution or protocol owner. */
export function useConversationScroll(scope: string, parts: ConversationPart[] | undefined, enabled: boolean, persistent: boolean) {
  const [checkpoints] = useState(createReadingCheckpoints)
  const scrollRef = useRef<HTMLDivElement>(null)
  const positions = useRef(new Map<string, Position>())
  const signatures = useRef(new Map<string, string>())
  const position = useRef(initialPosition())
  const currentScope = useRef<string | null>(null)
  const active = useRef(enabled)
  const canPersist = useRef(false)
  const ready = useRef(false)
  const appliedTop = useRef<number | null>(null)
  const geometry = useRef('')
  const [display, setDisplay] = useState({ unread: false, away: false })

  const publish = useCallback(() => {
    if (currentScope.current !== null) retain(positions.current, currentScope.current, position.current)
    const next = { unread: position.current.unread, away: !position.current.atBottom }
    setDisplay(previous => previous.unread === next.unread && previous.away === next.away ? previous : next)
  }, [])

  const persist = useCallback(() => {
    if (!canPersist.current || currentScope.current === null) return
    const { top, atBottom, readingAnchor } = position.current
    checkpoints.save(currentScope.current, { top, atBottom, anchor: atBottom ? null : readingAnchor })
  }, [checkpoints])

  useLayoutEffect(() => {
    const flushWhenHidden = () => { if (document.visibilityState === 'hidden') checkpoints.flush() }
    document.addEventListener('visibilitychange', flushWhenHidden)
    window.addEventListener('pagehide', checkpoints.flush)
    return () => {
      document.removeEventListener('visibilitychange', flushWhenHidden)
      window.removeEventListener('pagehide', checkpoints.flush)
      checkpoints.flush()
    }
  }, [checkpoints])

  const settle = useCallback(() => {
    const container = scrollRef.current
    if (!container || !active.current || !ready.current) return
    const saved = position.current
    if (saved.atBottom) container.scrollTop = container.scrollHeight
    else {
      const match = (attribute: 'conversationAnchor' | 'readingAnchor', target: Anchor | null) =>
        target && [...container.querySelectorAll<HTMLElement>('[data-conversation-anchor]')]
          .find(element => element.dataset[attribute] === target.key)
      const liveAnchor = match('conversationAnchor', saved.anchor)
      const anchor = liveAnchor ?? match('readingAnchor', saved.readingAnchor)
      const offset = liveAnchor ? saved.anchor?.top : saved.readingAnchor?.top
      if (anchor && offset !== undefined) {
        container.scrollTop += anchor.getBoundingClientRect().top - container.getBoundingClientRect().top - offset
      } else container.scrollTop = saved.top
    }
    // Do not remeasure an anchor after our own scroll: a resize can arrive in
    // several layouts, and the original canonical key/offset must survive all.
    appliedTop.current = container.scrollTop
    geometry.current = `${container.scrollHeight}/${container.clientHeight}/${container.clientWidth}`
  }, [])

  useLayoutEffect(() => {
    const changedScope = currentScope.current !== scope
    if (changedScope) {
      if (currentScope.current !== null) retain(positions.current, currentScope.current, position.current)
      currentScope.current = scope
      position.current = positions.current.get(scope) ?? initialPosition(persistent ? checkpoints.get(scope) : undefined)
    }
    active.current = enabled
    canPersist.current = persistent
    // An empty interim history during a switch is not a reading position or a
    // new message. Keep the saved anchor until canonical content arrives.
    ready.current = !!parts?.length
    if (parts?.length) {
      const next = signature(parts), previous = signatures.current.get(scope)
      if (previous && previous !== next && !position.current.atBottom) position.current = { ...position.current, unread: true }
      retain(signatures.current, scope, next)
    }
    if (enabled) settle()
    publish()
  }, [scope, parts, enabled, persistent, checkpoints, settle, publish])

  useLayoutEffect(() => {
    const container = scrollRef.current
    if (!container || !enabled) return
    let frame = 0
    let disposed = false
    const schedule = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => { if (!disposed && currentScope.current === scope) settle() })
    }
    const resize = new ResizeObserver(schedule)
    resize.observe(container)
    let content: Element | null = null
    const observeContent = () => {
      const next = container.querySelector('.wb-transcript')
      if (next === content) return
      if (content) resize.unobserve(content)
      content = next
      if (content) resize.observe(content)
    }
    observeContent()
    const observer = new MutationObserver(() => { observeContent(); schedule() })
    observer.observe(container, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ['open', 'class'] })
    window.addEventListener('resize', schedule)
    return () => {
      disposed = true
      cancelAnimationFrame(frame)
      observer.disconnect()
      resize.disconnect()
      window.removeEventListener('resize', schedule)
    }
  }, [scope, enabled, settle])

  const onScroll = useCallback(() => {
    const container = scrollRef.current
    if (!container || !active.current || !ready.current) return
    const nextGeometry = `${container.scrollHeight}/${container.clientHeight}/${container.clientWidth}`
    if (geometry.current !== nextGeometry) { settle(); return }
    if (appliedTop.current === container.scrollTop) return
    appliedTop.current = null
    const atBottom = container.scrollHeight - container.scrollTop - container.clientHeight <= BOTTOM_TOLERANCE
    position.current = { top: container.scrollTop, atBottom, ...measureAnchors(container), unread: atBottom ? false : position.current.unread }
    publish()
    persist()
  }, [publish, persist, settle])

  const jumpToLatest = useCallback(() => {
    if (!active.current || !ready.current) return
    // Instant navigation is deterministic and also honors reduced motion.
    position.current = initialPosition()
    settle()
    publish()
    persist()
  }, [publish, persist, settle])

  return { scrollRef, onScroll, unread: display.unread, away: display.away, jumpToLatest }
}
