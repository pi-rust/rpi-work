import { useState } from 'react'
import {
  Check,
  ChevronRight,
  CircleDashed,
  Loader2,
  Terminal,
  X,
} from 'lucide-react'

import {
  CodeBlock,
  CodeBlockCopyButton,
  CodeBlockHeader,
  CodeBlockTitle,
} from '@/components/reui/code-block/code-block'
import { Badge } from '@/components/ui/badge'
import { Separator } from '@/components/ui/separator'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'
import { toolMeta as toolMetaFor } from '@/data'
import { toolMeta } from '@/lib/viewmodel'
import type { Block, HistoryMessage, RunStep, ToolBlock } from '@/data'

export function UserRow({ message }: { message: HistoryMessage }) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">
        你
      </span>
      <p className="text-[15px] leading-7 whitespace-pre-wrap">{message.content}</p>
    </div>
  )
}

export function AnswerRow({ text, model }: { text: string; model: string }) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-muted-foreground flex items-center gap-2 text-[11px] font-medium tracking-wide uppercase">
        rpi
        <Badge variant="secondary" className="h-4 px-1.5 text-[10px] font-normal normal-case">
          {model.split('/')[1]}
        </Badge>
      </span>
      <p className="text-[15px] leading-7 whitespace-pre-wrap">{text}</p>
    </div>
  )
}

function StatusIcon({ status }: { status: ToolBlock['status'] }) {
  if (status === 'running') return <Loader2 className="size-3.5 animate-spin text-blue-500" />
  if (status === 'failed') return <X className="text-destructive size-3.5" />
  return <Check className="size-3.5 text-emerald-600" />
}

/** Tool card whose target, icon and read/shell kind all come from Rust. */
export function ToolCard({ block }: { block: ToolBlock }) {
  const meta = toolMeta(block.name, block.arguments)
  const detail = toolMetaFor[block.id]
  const [open, setOpen] = useState(meta.isRead)

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="bg-card overflow-hidden rounded-lg border">
        <CollapsibleTrigger className="hover:bg-muted/60 flex w-full items-center gap-3 px-3 py-2.5 text-left">
          <ChevronRight
            className={cn('text-muted-foreground size-3.5 transition-transform', open && 'rotate-90')}
          />
          <Terminal className="text-muted-foreground size-3.5" />
          <span className="text-[13px] font-medium">{block.name}</span>
          <span className="text-muted-foreground truncate font-mono text-[12px]">{meta.target}</span>
          <span className="text-muted-foreground ml-auto flex items-center gap-2 text-[11px] tabular-nums">
            {detail
              ? detail.durationMs >= 1000
                ? `${(detail.durationMs / 1000).toFixed(1)}s`
                : `${detail.durationMs}ms`
              : null}
            <StatusIcon status={block.status} />
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Separator />
          <div className="flex flex-col gap-3 p-3">
            <div className="flex flex-col gap-1">
              <span className="text-muted-foreground text-[11px]">
                {meta.isShell ? '命令' : '调用参数'}
              </span>
              <pre className="bg-muted/50 overflow-x-auto rounded-md px-2 py-1.5 font-mono text-[12px]">
                {block.arguments}
              </pre>
            </div>
            <div className="flex flex-col gap-1">
              <span className="text-muted-foreground text-[11px]">工具结果</span>
              <CodeBlock
                code={block.output}
                language={detail?.language ?? 'text'}
                showLineNumbers
              >
                <CodeBlockHeader>
                  <CodeBlockTitle>{meta.target}</CodeBlockTitle>
                  <span className="ml-auto" />
                  <CodeBlockCopyButton />
                </CodeBlockHeader>
              </CodeBlock>
            </div>
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  )
}

export function RunTimeline({ steps }: { steps: RunStep[] }) {
  return (
    <ol className="flex flex-col gap-2">
      {steps.map((step) => (
        <li key={step.id} className="flex items-center gap-3 text-[12px]">
          {step.status === 'done' ? (
            <Check className="size-3.5 shrink-0 text-emerald-600" />
          ) : (
            <CircleDashed className="text-muted-foreground size-3.5 shrink-0" />
          )}
          <span className="truncate">{step.title}</span>
          <span className="text-muted-foreground ml-auto tabular-nums">{step.at}</span>
        </li>
      ))}
    </ol>
  )
}

/** Resolve one wasm row reference back to its source data. */
export function rowElement(
  row: { source: 'history' | 'projection'; index: number },
  history: HistoryMessage[],
  blocks: Block[],
  model: string,
) {
  if (row.source === 'history') {
    const message = history[row.index]
    if (!message) return null
    return message.role === 'user' ? (
      <UserRow key={`h-${row.index}`} message={message} />
    ) : (
      <AnswerRow key={`h-${row.index}`} text={message.content} model={model} />
    )
  }
  const block = blocks[row.index]
  if (!block) return null
  return block.kind === 'text' ? (
    <AnswerRow key={`p-${row.index}`} text={block.text} model={model} />
  ) : (
    <ToolCard key={`p-${row.index}`} block={block} />
  )
}
