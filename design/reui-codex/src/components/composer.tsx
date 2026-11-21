import { ArrowUp, Paperclip, SlidersHorizontal } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  Avatar,
  AvatarFallback,
} from '@/components/ui/avatar'
import { node, type Metrics } from '@/data'

type Props = {
  draft: string
  onDraft: (value: string) => void
  metrics: Metrics
}

/** Codex-style composer: rounded card, model chip row, send button. */
export function Composer({ draft, onDraft, metrics }: Props) {
  return (
    <div className="bg-background/80 sticky bottom-0 px-6 pt-2 pb-3 backdrop-blur">
      <div className="border-input bg-card focus-within:border-ring/60 mx-auto flex max-w-3xl flex-col gap-2 rounded-xl border p-3 shadow-sm">
        <Textarea
          value={draft}
          onChange={(event) => onDraft(event.target.value)}
          placeholder="给 rpi 发消息…（原型：只保存在内存）"
          className="max-h-40 min-h-[52px] resize-none border-0 bg-transparent p-0 text-[14px] shadow-none focus-visible:ring-0"
        />
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="icon" className="size-7" aria-label="添加附件">
            <Paperclip className="size-4" />
          </Button>
          <Button variant="ghost" size="sm" className="h-7 gap-1.5 px-2 text-[12px]">
            <Avatar className="size-4">
              <AvatarFallback className="text-[8px]">r</AvatarFallback>
            </Avatar>
            {node.model.split('/')[1]}
          </Button>
          <Button variant="ghost" size="sm" className="h-7 gap-1.5 px-2 text-[12px]">
            <SlidersHorizontal className="size-3.5" />
            思考 {node.thinking}
          </Button>
          <span className="text-muted-foreground ml-auto text-[11px] tabular-nums">
            上下文 {metrics.contextPercent}%
          </span>
          <Button
            size="icon"
            className="size-7 rounded-full"
            aria-label="发送"
            disabled={draft.trim().length === 0}
          >
            <ArrowUp className="size-4" />
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mt-2 text-center text-[11px]">
        原型：不发送到节点 · 只渲染 fixture 数据
      </p>
    </div>
  )
}
