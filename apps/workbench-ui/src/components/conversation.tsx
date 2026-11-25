import { useState } from 'react'
import { ChevronRight, GitFork, ThumbsDown, ThumbsUp } from 'lucide-react'
import { AnswerRow, ToolCard, UserRow } from '@/components/transcript'
import type { ConversationPart, ConversationView } from '@/lib/viewmodel'
import type { ToolDetail } from '@/lib/types'
import { useReplyRatings } from '@/lib/reply-ratings'

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
    <details className="wb-activity min-w-0" open={open} data-activity-key={part.key}>
      <summary className="wb-activity-summary text-muted-foreground cursor-pointer focus-visible:outline-2 focus-visible:outline-ring"
        onClick={(event) => { event.preventDefault(); setChoice(!open); remember(choices, `${part.key}/choice`, !open) }}>
        <ChevronRight aria-hidden className={`size-3.5 shrink-0 transition-transform ${open ? 'rotate-90' : ''}`} />
        <span>{part.label}</span>
        <span className="tabular-nums">{part.items.length} 项</span>
      </summary>
      <div className="wb-activity-body flex min-w-0 flex-col gap-2">
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

export function ConversationObservation({ view }: { view: ConversationView }) {
  return <p className={`wb-observation ${observationClasses[view.observationTone]}`} data-tone={view.observationTone} role="status" aria-label="会话运行观察">会话观察：{view.observation}</p>
}

/** Parts already contain canonical identity, history/tool normalization and grouping. */
export function Conversation({ view, details, choices, onFork, forkDisabled, ratingScope }: {
  view: ConversationView; details?: Record<string, ToolDetail>; choices: Choices
  onFork?: (entryId: string) => void; forkDisabled?: boolean; ratingScope?: string
}) {
  const { ratings, toggleRating } = useReplyRatings(ratingScope)
  return <>
    {view.parts.map((part) => {
      if (part.kind === 'activity') return (
        <div key={part.key} className="wb-conversation-anchor min-w-0" data-conversation-anchor={part.key}>
          <ActivityBlock part={part} details={details} choices={choices} />
        </div>
      )
      const ratingId = ratingScope ? part.ratingEntryId : undefined
      const rating = ratingId ? ratings.get(ratingId) : undefined
      const forkId = part.forkEntryId
      return (
        <div key={part.key} className="wb-conversation-anchor min-w-0" data-conversation-anchor={part.key} data-reading-anchor={part.readingKey}>
          {part.role === 'user' ? <UserRow message={{ id: part.key, role: part.role, content: part.text }} /> : <AnswerRow text={part.text} action={ratingId || (onFork && forkId) ? <span className="wb-copy-control">
            {ratingId ? <>
              <button type="button" aria-label="有帮助" title="有帮助（仅本设备）" aria-pressed={rating === 'helpful'} onClick={() => toggleRating(ratingId, 'helpful')}><ThumbsUp aria-hidden="true" /></button>
              <button type="button" aria-label="没有帮助" title="没有帮助（仅本设备）" aria-pressed={rating === 'unhelpful'} onClick={() => toggleRating(ratingId, 'unhelpful')}><ThumbsDown aria-hidden="true" /></button>
            </> : null}
            {onFork && forkId ? <button type="button" aria-label="从此处开始新对话" title="从此处开始新对话" disabled={forkDisabled} onClick={() => onFork(forkId)}><GitFork aria-hidden="true" /></button> : null}
          </span> : undefined} />}
        </div>
      )
    })}
  </>
}
