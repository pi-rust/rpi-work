import { useState } from 'react'
import { ChevronRight } from 'lucide-react'
import { AnswerRow, ToolCard, UserRow } from '@/components/transcript'
import type { ConversationPart, ConversationView } from '@/lib/viewmodel'
import type { ToolDetail } from '@/lib/types'

type Activity = Extract<ConversationPart, { kind: 'activity' }>
type Choices = Map<string, boolean>

function remember(choices: Choices, key: string, value: boolean) {
  choices.delete(key)
  choices.set(key, value)
  if (choices.size > 512) choices.delete(choices.keys().next().value!)
}

/** Only user display intent lives here; phase/defaults are Rust observations. */
function ActivityBlock({ part, details, choices }: { part: Activity; details?: Record<string, ToolDetail>; choices: Choices }) {
  const [choice, setChoice] = useState<boolean | null>(() => choices.get(`${part.key}/choice`) ?? null)
  const [childOpened, setChildOpened] = useState(() => choices.get(`${part.key}/child`) ?? false)
  const [folded, setFolded] = useState(() => choices.get(`${part.key}/folded`) ?? part.collapsed)
  // A completed display segment stays folded when another run starts; an
  // actual running/failed/unknown item still requests attention via Rust.
  if (!choices.has(`${part.key}/folded`)) remember(choices, `${part.key}/folded`, folded)
  if (part.collapsed && !folded) { setFolded(true); remember(choices, `${part.key}/folded`, true) }
  const open = choice ?? (childOpened || part.attention || !folded)

  return (
    <details className="wb-activity min-w-0 rounded-xl border" open={open} data-activity-key={part.key}>
      <summary className="text-muted-foreground flex cursor-pointer items-center gap-2 px-3 py-2 text-[12px] focus-visible:outline-2 focus-visible:outline-ring"
        onClick={(event) => { event.preventDefault(); setChoice(!open); remember(choices, `${part.key}/choice`, !open) }}>
        <ChevronRight aria-hidden className={`size-3.5 shrink-0 transition-transform ${open ? 'rotate-90' : ''}`} />
        <span>{part.label}</span>
        <span className="ml-auto tabular-nums">{part.items.length} 项</span>
      </summary>
      <div className="wb-activity-body flex min-w-0 flex-col gap-3 border-t p-3">
        {part.items.map((item) => item.kind === 'tool' ? (
          <ToolCard key={item.key} block={item.block} detail={details?.[item.block.id]} initialOpen={choices.get(item.key)}
            onIntent={(expanded) => {
              remember(choices, item.key, expanded)
              if (expanded) { setChildOpened(true); remember(choices, `${part.key}/child`, true) }
            }} />
        ) : (
          <details key={item.key} className="min-w-0 rounded-lg border">
            <summary className="text-muted-foreground cursor-pointer px-3 py-2 text-[12px]">{item.label}</summary>
            <p className="text-muted-foreground px-3 pb-3 text-[13px] leading-6 whitespace-pre-wrap break-words">{item.text}</p>
          </details>
        ))}
      </div>
    </details>
  )
}

const observationClasses = {
  muted: 'text-muted-foreground', active: 'text-blue-600 dark:text-blue-400',
  success: 'text-emerald-700 dark:text-emerald-400', danger: 'text-destructive',
  warning: 'text-amber-700 dark:text-amber-400',
}

/** Parts already contain canonical identity, history/tool normalization and grouping. */
export function Conversation({ view, details, observed = true, choices }: { view: ConversationView; details?: Record<string, ToolDetail>; observed?: boolean; choices: Choices }) {
  return <>
    {observed ? <p className={`wb-observation text-[12px] ${observationClasses[view.observationTone]}`} data-tone={view.observationTone} role="status" aria-label="会话运行观察">会话观察：{view.observation}</p> : null}
    {view.parts.map((part) => part.kind === 'activity' ? (
      <div key={part.key} className="wb-conversation-anchor min-w-0" data-conversation-anchor={part.key}>
        <ActivityBlock part={part} details={details} choices={choices} />
      </div>
    ) : part.role === 'user' ? (
      <div key={part.key} className="wb-conversation-anchor min-w-0" data-conversation-anchor={part.key}>
        <UserRow message={{ id: part.key, role: part.role, content: part.text }} />
      </div>
    ) : (
      <div key={part.key} className="wb-conversation-anchor min-w-0" data-conversation-anchor={part.key}>
        <AnswerRow text={part.text} />
      </div>
    ))}
  </>
}
