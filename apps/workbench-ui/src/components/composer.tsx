import { useLayoutEffect, useRef, type ReactNode } from 'react'
import { ArrowUp, Plus, Square, X } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import type { ContextView } from '@/lib/viewmodel'
import type { DraftSelection, FeedbackItem } from '@/lib/drafts'
import './composer.css'

type Props = {
  draft: string
  selection?: DraftSelection
  feedback?: FeedbackItem[]
  onRemoveFeedback?: (id: string) => void
  onDraft: (value: string) => void
  context: ContextView
  observation?: ReactNode
  onSend: () => void
  onAbort: () => void
  onQueue?: () => void
  sendDisabledReason?: string
  queueDisabledReason?: string
  abortDisabled?: boolean
  modelDisabled?: boolean
  thinkingDisabled?: boolean
  selectedModel?: { id: string; provider: string }
  busy: boolean
  error: string | null
  modelLabel: string
  thinking: string
  modelOptions?: { id: string; label: string; provider: string }[]
  thinkingOptions?: string[]
  onModel?: (id: string, provider: string) => void
  onThinking?: (level: string) => void
}

export function Composer({
  draft,
  selection,
  feedback,
  onRemoveFeedback,
  onDraft,
  context,
  observation,
  onSend,
  onAbort,
  onQueue,
  sendDisabledReason,
  queueDisabledReason,
  abortDisabled = true,
  modelDisabled = true,
  thinkingDisabled = true,
  selectedModel,
  busy,
  error,
  modelLabel,
  thinking,
  modelOptions,
  thinkingOptions,
  onModel,
  onThinking,
}: Props) {
  // IME candidates must not submit: only a committed composition can send.
  const composing = useRef(false)
  const textarea = useRef<HTMLTextAreaElement>(null)
  useLayoutEffect(() => {
    if (selection?.text === draft && !composing.current) textarea.current?.setSelectionRange(selection.start, selection.end, selection.direction)
  }, [draft, selection])
  const rememberSelection = (node: HTMLTextAreaElement) => {
    if (selection) Object.assign(selection, { text: node.value, start: node.selectionStart, end: node.selectionEnd, direction: node.selectionDirection })
  }
  return (
    <div className="wb-compose-area">
      <div className="wb-composer" role="group" aria-label="消息输入">
        {feedback?.length ? <ul aria-label="行内反馈草稿" className="flex max-h-24 min-w-0 flex-wrap gap-1.5 overflow-y-auto p-0.5">
          {feedback.map(item => <li key={item.id} className="bg-muted flex min-w-0 max-w-full items-center gap-1 rounded-md pl-2 text-xs">
            <span className="min-w-0 truncate" title={`${item.location}\n${item.comment}`}>{item.location}</span>
            <Button variant="ghost" size="icon" className="size-7 shrink-0 max-md:size-10" aria-label={`移除 ${item.location} 的反馈`}
              onClick={() => { onRemoveFeedback?.(item.id); textarea.current?.focus({ preventScroll: true }) }}><X aria-hidden="true" /></Button>
          </li>)}
        </ul> : null}
        <Textarea
          ref={textarea}
          value={draft}
          onSelect={(event) => rememberSelection(event.currentTarget)}
          onBlur={(event) => rememberSelection(event.currentTarget)}
          onChange={(event) => { rememberSelection(event.currentTarget); onDraft(event.target.value) }}
          onCompositionStart={() => {
            composing.current = true
          }}
          onCompositionEnd={() => {
            composing.current = false
          }}
          onKeyDown={(event) => {
            if (composing.current || event.nativeEvent.isComposing) return
            if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
              event.preventDefault()
              if (!sendDisabledReason && draft.trim()) onSend()
            }
          }}
          aria-label="消息"
          placeholder="给 rpi 发消息…"
          className="wb-composer-input"
        />
        {error ? <p className="text-destructive px-1 py-1 text-[11px]">{error}</p> : null}
        <div className="wb-composer-tools">
          <Button variant="ghost" size="icon" className="wb-composer-add" aria-label="添加附件" disabled>
            <Plus aria-hidden="true" />
          </Button>
          <span className="wb-composer-select">
            {modelOptions && modelOptions.length > 0 && onModel ? (
              <select
                aria-label="选择模型"
                value={selectedModel ? JSON.stringify([selectedModel.provider, selectedModel.id]) : ''}
                disabled={modelDisabled}
                onChange={(event) => {
                  const option = modelOptions.find((entry) => JSON.stringify([entry.provider, entry.id]) === event.target.value)
                  if (option) onModel(option.id, option.provider)
                }}
              >
                <option value="" disabled>{modelLabel}</option>
                {modelOptions.map((option) => (
                  <option key={JSON.stringify([option.provider, option.id])} value={JSON.stringify([option.provider, option.id])}>
                    {option.label}
                  </option>
                ))}
              </select>
            ) : (
              <span title={modelLabel}>{modelLabel}</span>
            )}
          </span>
          <span className="wb-composer-select">
            {thinkingOptions && thinkingOptions.length > 0 && onThinking ? (
              <select
                aria-label="选择思考等级"
                value={thinking}
                disabled={thinkingDisabled}
                onChange={(event) => onThinking(event.target.value)}
              >
                {thinkingOptions.map((level) => (
                  <option key={level} value={level}>
                    思考 {level}
                  </option>
                ))}
              </select>
            ) : (
              <span>{`思考 ${thinking}`}</span>
            )}
          </span>
          {onQueue ? (
            <Button
              variant="ghost"
              size="sm"
              className="wb-composer-queue"
              onClick={onQueue}
              disabled={!!queueDisabledReason || draft.trim().length === 0}
              title={queueDisabledReason}
            >
              加入队列
            </Button>
          ) : null}
          {busy ? (
            <Button size="icon" className="wb-composer-send" aria-label="中止" disabled={abortDisabled} onClick={onAbort} variant="secondary">
              <Square className="size-3.5" />
            </Button>
          ) : (
            <Button
              size="icon"
              className="wb-composer-send"
              aria-label="发送"
              disabled={!!sendDisabledReason || draft.trim().length === 0}
              title={sendDisabledReason}
              onClick={onSend}
              aria-keyshortcuts="Meta+Enter Control+Enter"
            >
              <ArrowUp className="size-4" />
            </Button>
          )}
        </div>
      </div>
      <div className="wb-composer-context">
        {observation}
        <span className="wb-composer-usage">上下文 {context.percentText}</span>
      </div>
    </div>
  )
}
