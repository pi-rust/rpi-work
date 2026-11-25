import { useEffect, useMemo, useRef, useState } from 'react'
import { Search } from 'lucide-react'

import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { Project, Session } from '@/lib/types'

type Props = {
  open: boolean
  projects: Project[]
  sessions: Session[]
  onClose: () => void
  onSelect: (sessionId: string) => void
}

type Entry = { id: string; label: string; hint: string }

/**
 * Command palette (⌘K): one keyboard path to any project or session.
 *
 * Sessions are filtered by title and project name; Enter opens the highlighted
 * one, Escape closes, and the list is a plain listbox so screen readers get the
 * active option.
 */
export function CommandPalette({ open, projects, sessions, onClose, onSelect }: Props) {
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const input = useRef<HTMLInputElement | null>(null)

  const entries = useMemo<Entry[]>(() => {
    const names = new Map(projects.map((project) => [project.id, project.name]))
    return sessions
      .filter(session => !session.archived)
      .map((session) => ({
        id: session.id,
        label: session.title,
        hint: names.get(session.projectId) ?? session.projectId,
      }))
      .filter((entry) => {
        const needle = query.trim().toLowerCase()
        if (needle.length === 0) return true
        return `${entry.label} ${entry.hint}`.toLowerCase().includes(needle)
      })
      .slice(0, 12)
  }, [projects, sessions, query])

  useEffect(() => {
    if (!open) return
    setQuery('')
    setActive(0)
    const timer = setTimeout(() => input.current?.focus(), 0)
    return () => clearTimeout(timer)
  }, [open])

  useEffect(() => {
    setActive((current) => (entries.length === 0 ? 0 : Math.min(current, entries.length - 1)))
  }, [entries.length])

  if (!open) return null

  const choose = (entry: Entry | undefined) => {
    if (!entry) return
    onSelect(entry.id)
    onClose()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[12vh]" role="presentation">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="搜索项目与会话"
        className="bg-card w-[min(560px,92vw)] overflow-hidden rounded-xl border shadow-2xl"
      >
        <div className="flex items-center gap-2 border-b px-3 py-2">
          <Search className="text-muted-foreground size-4" />
          <Input
            ref={input}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault()
                onClose()
              } else if (event.key === 'ArrowDown') {
                event.preventDefault()
                setActive((current) => (entries.length === 0 ? 0 : (current + 1) % entries.length))
              } else if (event.key === 'ArrowUp') {
                event.preventDefault()
                setActive((current) => (entries.length === 0 ? 0 : (current - 1 + entries.length) % entries.length))
              } else if (event.key === 'Enter') {
                event.preventDefault()
                choose(entries[active])
              }
            }}
            placeholder="搜索会话或项目…"
            aria-label="搜索项目与会话"
            aria-controls="command-palette-list"
            className="h-7 border-0 bg-transparent text-[13px] shadow-none focus-visible:ring-0"
          />
          <kbd className="text-muted-foreground text-[10px]">Esc</kbd>
        </div>
        <ul id="command-palette-list" role="listbox" aria-label="匹配的会话" className="max-h-[42vh] overflow-y-auto p-1">
          {entries.length === 0 ? (
            <li className="text-muted-foreground px-2 py-3 text-[12px]">没有匹配的会话</li>
          ) : (
            entries.map((entry, index) => (
              <li
                key={entry.id}
                role="option"
                aria-selected={index === active}
                onMouseEnter={() => setActive(index)}
                onClick={() => choose(entry)}
                className={cn(
                  'flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-[12px]',
                  index === active ? 'bg-muted text-foreground' : 'text-muted-foreground',
                )}
              >
                <span className="truncate">{entry.label}</span>
                <span className="text-muted-foreground/70 ml-auto shrink-0 text-[10px]">{entry.hint}</span>
              </li>
            ))
          )}
        </ul>
      </div>
    </div>
  )
}
