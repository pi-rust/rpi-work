import { useLayoutEffect, useRef } from 'react'
import { ArrowUp, Paperclip, SlidersHorizontal, Square } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  Avatar,
  AvatarFallback,
} from '@/components/ui/avatar'
import type { ContextView } from '@/lib/viewmodel'
import type { DraftSelection } from '@/lib/drafts'

type Props = {
  draft: string
  selection?: DraftSelection
  onDraft: (value: string) => void
  context: ContextView
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

/** Codex-style composer: rounded card, model chip row, send button. */
export function Composer({
  draft,
  selection,
  onDraft,
  context,
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
    <div className="bg-background/80 sticky bottom-0 px-3 pt-2 pb-2 backdrop-blur md:px-6 md:pb-3">
      <div className="border-input bg-card focus-within:border-ring/60 mx-auto flex max-w-3xl flex-col gap-2 rounded-xl border p-2.5 shadow-sm md:p-3">
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
              if (!sendDisabledReason) onSend()
            }
          }}
          placeholder="给 rpi 发消息…（⌘↵ 发送）"
          className="max-h-40 min-h-[36px] resize-none border-0 bg-transparent p-0 text-[14px] shadow-none focus-visible:ring-0 md:min-h-[52px]"
        />
        {error ? <p className="text-destructive text-[11px]">{error}</p> : null}
        <div className="flex flex-wrap items-center gap-1.5 md:gap-2">
          <Button variant="ghost" size="icon" className="size-7" aria-label="添加附件">
            <Paperclip className="size-4" />
          </Button>
          <span className="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px]">
            <Avatar className="size-4">
              <AvatarFallback className="text-[8px]">r</AvatarFallback>
            </Avatar>
            {modelOptions && modelOptions.length > 0 && onModel ? (
              <select
                aria-label="选择模型"
                value={selectedModel ? JSON.stringify([selectedModel.provider, selectedModel.id]) : ''}
                disabled={modelDisabled}
                onChange={(event) => {
                  const option = modelOptions.find((entry) => JSON.stringify([entry.provider, entry.id]) === event.target.value)
                  if (option) onModel(option.id, option.provider)
                }}
                className="bg-transparent outline-none"
              >
                <option value="" disabled>{modelLabel}</option>
                {modelOptions.map((option) => (
                  <option key={JSON.stringify([option.provider, option.id])} value={JSON.stringify([option.provider, option.id])}>
                    {option.label}
                  </option>
                ))}
              </select>
            ) : (
              modelLabel
            )}
          </span>
          <span className="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px]">
            <SlidersHorizontal className="size-3.5" />
            {thinkingOptions && thinkingOptions.length > 0 && onThinking ? (
              <select
                aria-label="选择思考等级"
                value={thinking}
                disabled={thinkingDisabled}
                onChange={(event) => onThinking(event.target.value)}
                className="bg-transparent outline-none"
              >
                {thinkingOptions.map((level) => (
                  <option key={level} value={level}>
                    思考 {level}
                  </option>
                ))}
              </select>
            ) : (
              `思考 ${thinking}`
            )}
          </span>
          {onQueue ? (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-[11px]"
              onClick={onQueue}
              disabled={!!queueDisabledReason || draft.trim().length === 0}
              title={queueDisabledReason}
            >
              加入队列
            </Button>
          ) : null}
          <span className="text-muted-foreground ml-auto text-[11px] tabular-nums">
            上下文 {context.percentText}
          </span>
          {busy ? (
            <Button size="icon" className="size-7 rounded-full" aria-label="中止" disabled={abortDisabled} onClick={onAbort} variant="secondary">
              <Square className="size-3.5" />
            </Button>
          ) : (
            <Button
              size="icon"
              className="size-7 rounded-full"
              aria-label="发送"
              disabled={!!sendDisabledReason || draft.trim().length === 0}
              title={sendDisabledReason}
              onClick={onSend}
              aria-keyshortcuts="Meta+Enter"
            >
              <ArrowUp className="size-4" />
            </Button>
          )}
        </div>
      </div>
      <p className="text-muted-foreground mt-1 text-center text-[10px] md:mt-2 md:text-[11px]">
        原型：⌘↵ 发送 · 事件经 Rust 投影渲染
      </p>
    </div>
  )
}
