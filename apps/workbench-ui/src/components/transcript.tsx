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
import { RichAnswer } from '@/components/rich-answer'
import { toolMeta } from '@/lib/viewmodel'
import type { HistoryMessage, RunStep, ToolBlock, ToolDetail } from '@/lib/types'
import './transcript.css'

export function UserRow({ message }: { message: HistoryMessage }) {
  // Codex keeps the prompt visually distinct: a compact, right-aligned chip that
  // never spans the reading column.
  return (
    <div className="wb-user-row flex flex-col items-end gap-1">
      <span className="wb-user-label text-muted-foreground text-[11px] font-medium tracking-wide uppercase">你</span>
      <p className="wb-prompt bg-muted/70 max-w-[85%] rounded-2xl rounded-tr-sm px-3.5 py-2 text-[15px] leading-7 whitespace-pre-wrap">
        {message.content}
      </p>
    </div>
  )
}

export function AnswerRow({ text, model }: { text: string; model?: string }) {
  // Assistant prose owns the reading column; tool rows sit underneath at the same
  // left edge so a turn reads as one block.
  return (
    <div className="wb-answer flex min-w-0 flex-col gap-2">
      <span className="text-muted-foreground flex items-center gap-2 text-[11px] font-medium tracking-wide uppercase">
        rpi
        {model ? <Badge variant="secondary" className="h-4 px-1.5 text-[10px] font-normal normal-case">
          {model.split('/').at(-1)}
        </Badge> : null}
      </span>
      <RichAnswer text={text} />
    </div>
  )
}

function StatusIcon({ status }: { status: ToolBlock['status'] }) {
  if (status === 'running') return <Loader2 className="size-3.5 animate-spin text-blue-500" />
  if (status === 'failed') return <X className="text-destructive size-3.5" />
  if (status === 'completed') return <Check className="size-3.5 text-emerald-600" />
  return <CircleDashed className="text-muted-foreground size-3.5" />
}

/** Tool card whose target, icon and read/shell kind all come from Rust. */
export function ToolCard({ block, detail, onIntent, initialOpen }: { block: Omit<ToolBlock, 'kind'>; detail?: ToolDetail; onIntent?: (open: boolean) => void; initialOpen?: boolean }) {
  const meta = toolMeta(block.name, block.arguments)
  const [open, setOpen] = useState(initialOpen ?? meta.isRead)

  return (
    <Collapsible open={open} onOpenChange={(value) => { setOpen(value); onIntent?.(value) }}>
      <div data-tool-id={block.id} className="wb-tool-card bg-card overflow-hidden rounded-lg border">
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
            <span className="sr-only">{block.statusLabel}</span>
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
                // Both sides resolve to the CSS-variable theme, so token colours
                // come from the Rust palette instead of a bundled shiki theme.
                themes={{ light: 'css-variables', dark: 'css-variables' }}
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
