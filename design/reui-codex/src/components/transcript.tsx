import { useState } from 'react'
import { Check, ChevronRight, CircleDashed, Loader2, Terminal, X } from 'lucide-react'

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
import type { ToolCall, Turn } from '@/data'

export function MessageRow({ turn }: { turn: Turn }) {
  if (turn.role === 'user') {
    return (
      <div className="flex flex-col gap-2">
        <span className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">你</span>
        <p className="text-[15px] leading-7 whitespace-pre-wrap">{turn.text}</p>
      </div>
    )
  }
  return (
    <div className="flex flex-col gap-2">
      <span className="text-muted-foreground flex items-center gap-2 text-[11px] font-medium tracking-wide uppercase">
        rpi
        <Badge variant="secondary" className="h-4 px-1.5 text-[10px] font-normal normal-case">
          deepseek-v4-pro
        </Badge>
      </span>
      <p className="text-[15px] leading-7 whitespace-pre-wrap">{turn.text}</p>
    </div>
  )
}

function StatusIcon({ status }: { status: ToolCall['status'] }) {
  if (status === 'running') return <Loader2 className="size-3.5 animate-spin text-blue-500" />
  if (status === 'failed') return <X className="size-3.5 text-destructive" />
  return <Check className="size-3.5 text-emerald-600" />
}

export function ToolCard({ tool }: { tool: ToolCall }) {
  const [open, setOpen] = useState(tool.name === 'read')

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="bg-card overflow-hidden rounded-lg border">
        <CollapsibleTrigger className="hover:bg-muted/60 flex w-full items-center gap-3 px-3 py-2.5 text-left">
          <ChevronRight
            className={cn('text-muted-foreground size-3.5 transition-transform', open && 'rotate-90')}
          />
          <Terminal className="text-muted-foreground size-3.5" />
          <span className="text-[13px] font-medium">{tool.name}</span>
          <span className="text-muted-foreground truncate font-mono text-[12px]">{tool.target}</span>
          <span className="text-muted-foreground ml-auto flex items-center gap-2 text-[11px] tabular-nums">
            {tool.durationMs >= 1000 ? `${(tool.durationMs / 1000).toFixed(1)}s` : `${tool.durationMs}ms`}
            <StatusIcon status={tool.status} />
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Separator />
          <div className="flex flex-col gap-3 p-3">
            <div className="flex flex-col gap-1">
              <span className="text-muted-foreground text-[11px]">调用参数</span>
              <pre className="bg-muted/50 overflow-x-auto rounded-md px-2 py-1.5 font-mono text-[12px]">
                {tool.detail}
              </pre>
            </div>
            <div className="flex flex-col gap-1">
              <span className="text-muted-foreground text-[11px]">工具结果</span>
              <CodeBlock code={tool.output} language={tool.language} showLineNumbers>
                <CodeBlockHeader>
                  <CodeBlockTitle>{tool.target}</CodeBlockTitle>
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

export function RunTimeline({ steps }: { steps: { id: string; title: string; status: string; at: string }[] }) {
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
