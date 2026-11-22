import { ArrowUp, Paperclip, SlidersHorizontal, Square } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  Avatar,
  AvatarFallback,
} from '@/components/ui/avatar'
import { node } from '@/data'
import type { ContextView } from '@/lib/viewmodel'

type Props = {
  draft: string
  onDraft: (value: string) => void
  context: ContextView
  onSend: () => void
  onAbort: () => void
  onQueue?: () => void
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
  onDraft,
  context,
  onSend,
  onAbort,
  onQueue,
  busy,
  error,
  modelLabel,
  thinking,
  modelOptions,
  thinkingOptions,
  onModel,
  onThinking,
}: Props) {
  return (
    <div className="bg-background/80 sticky bottom-0 px-3 pt-2 pb-2 backdrop-blur md:px-6 md:pb-3">
      <div className="border-input bg-card focus-within:border-ring/60 mx-auto flex max-w-3xl flex-col gap-2 rounded-xl border p-2.5 shadow-sm md:p-3">
        <Textarea
          value={draft}
          onChange={(event) => onDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
              event.preventDefault()
              onSend()
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
                value={modelOptions.find((option) => option.label === modelLabel)?.id ?? modelOptions[0].id}
                onChange={(event) => {
                  const option = modelOptions.find((entry) => entry.id === event.target.value)
                  if (option) onModel(option.id, option.provider)
                }}
                className="bg-transparent outline-none"
              >
                {modelOptions.map((option) => (
                  <option key={option.id} value={option.id}>
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
              disabled={draft.trim().length === 0}
            >
              加入队列
            </Button>
          ) : null}
          <span className="text-muted-foreground ml-auto text-[11px] tabular-nums">
            上下文 {context.percentText}
          </span>
          {busy ? (
            <Button size="icon" className="size-7 rounded-full" aria-label="中止" onClick={onAbort} variant="secondary">
              <Square className="size-3.5" />
            </Button>
          ) : (
            <Button
              size="icon"
              className="size-7 rounded-full"
              aria-label="发送"
              disabled={draft.trim().length === 0}
              onClick={onSend}
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
