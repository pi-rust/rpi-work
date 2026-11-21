import { Moon, PanelRight, Sun } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Separator } from '@/components/ui/separator'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { formatTokens, metrics, node, runSteps, sessions, tools, turns } from '@/data'
import { Composer } from '@/components/composer'
import { MessageRow, RunTimeline, ToolCard } from '@/components/transcript'
import { Sidebar } from '@/components/workspace-sidebar'
import { useMemo, useState } from 'react'

export default function App() {
  const [project, setProject] = useState('rpi-work')
  const [session, setSession] = useState('dark')
  const [draft, setDraft] = useState('')
  const [dark, setDark] = useState(false)
  const [rail, setRail] = useState(true)

  const title = useMemo(
    () => sessions.find((entry) => entry.id === session)?.title ?? '新对话',
    [session],
  )

  return (
    <div className={dark ? 'dark h-dvh' : 'h-dvh'}>
      <div className="bg-background text-foreground flex h-full overflow-hidden">
        <Sidebar
          projects={[
            { id: 'rpi-work', name: 'rpi-work' },
            { id: 'pi-rust', name: 'pi-rust' },
          ]}
          sessions={sessions}
          project={project}
          session={session}
          onProject={setProject}
          onSession={setSession}
          onNewChat={() => setDraft('')}
        />

        <main className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-11 shrink-0 items-center gap-3 border-b px-4">
            <span className="text-muted-foreground text-[13px]">{project}</span>
            <span className="text-muted-foreground/50 text-[13px]">/</span>
            <span className="truncate text-[13px] font-medium">{title}</span>
            <Badge variant="outline" className="ml-1 h-5 gap-1 px-1.5 text-[10px] font-normal">
              <span className="size-1.5 rounded-full bg-emerald-500" />
              在线
            </Badge>
            <div className="ml-auto flex items-center gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="size-7"
                aria-label="切换主题"
                onClick={() => setDark((value) => !value)}
              >
                {dark ? <Sun className="size-4" /> : <Moon className="size-4" />}
              </Button>
              <Button
                variant={rail ? 'secondary' : 'ghost'}
                size="icon"
                className="size-7"
                aria-label="运行面板"
                onClick={() => setRail((value) => !value)}
              >
                <PanelRight className="size-4" />
              </Button>
            </div>
          </header>

          <div className="flex min-h-0 flex-1">
            <div className="flex min-w-0 flex-1 flex-col">
              <div className="min-h-0 flex-1 overflow-y-auto px-6 py-6">
                <div className="mx-auto flex max-w-3xl flex-col gap-6">
                  {turns.map((turn) => (
                    <MessageRow key={turn.id} turn={turn} />
                  ))}
                  <div className="flex flex-col gap-2">
                    {tools.map((tool) => (
                      <ToolCard key={tool.id} tool={tool} />
                    ))}
                  </div>
                </div>
              </div>
              <Composer draft={draft} onDraft={setDraft} metrics={metrics} />
            </div>

            {rail ? (
              <aside className="w-[300px] shrink-0 border-l">
                <div className="flex h-full flex-col">
                  <div className="flex h-11 items-center justify-between border-b px-3 text-[12px] font-medium">
                    运行
                    <span className="text-muted-foreground font-normal">fixture</span>
                  </div>
                  <div className="min-h-0 flex-1 overflow-y-auto p-3">
                    <RunTimeline steps={runSteps} />
                    <Separator className="my-3" />
                    <div className="flex flex-col gap-2 text-[12px]">
                      <Row label="模型" value={node.model} />
                      <Row label="思考" value={node.thinking} />
                      <Row label="工具" value={`${metrics.tools} 次 · 失败 ${metrics.failures}`} />
                      <Row label="缓存命中" value={`${metrics.cachePercent}%`} />
                      <Row label="成本" value={`$${metrics.cost.toFixed(4)}`} />
                    </div>
                  </div>
                  <div className="text-muted-foreground border-t px-3 py-2 text-[11px]">
                    上下文 {formatTokens(metrics.contextUsed)} / {formatTokens(metrics.contextWindow)}
                  </div>
                </div>
              </aside>
            ) : null}
          </div>

          <footer className="text-muted-foreground flex h-7 shrink-0 items-center gap-4 border-t px-4 text-[11px] tabular-nums">
            <span>↑ {formatTokens(metrics.input)}</span>
            <span>↓ {formatTokens(metrics.output)}</span>
            <span>缓存 {metrics.cachePercent}%</span>
            <span>成本 ${metrics.cost.toFixed(4)}</span>
            <span>上下文 {metrics.contextPercent}%</span>
            <span>工具 {metrics.tools} / 失败 {metrics.failures}</span>
            <Tooltip>
              <TooltipTrigger className="ml-auto cursor-default">
                ReUI · Codex 布局原型
              </TooltipTrigger>
              <TooltipContent>MIT 许可的 ReUI 组件 + work-viewmodel 里的数据</TooltipContent>
            </Tooltip>
          </footer>
        </main>
      </div>
    </div>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="truncate font-mono text-[11px]">{value}</span>
    </div>
  )
}
