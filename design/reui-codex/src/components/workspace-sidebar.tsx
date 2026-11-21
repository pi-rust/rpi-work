import { ChevronDown, Dot, Plus, Search, Settings } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Separator } from '@/components/ui/separator'
import { cn } from '@/lib/utils'
import type { ReactNode } from 'react'
import type { Project, Session } from '@/data'

type Props = {
  projects: Project[]
  sessions: Session[]
  project: string
  session: string
  onProject: (id: string) => void
  onSession: (id: string) => void
  onNewChat: () => void
  className?: string
  /** Connection controls, rendered above the node row. */
  connection?: ReactNode
  nodeLabel?: string
  nodeState?: 'fixture' | 'online' | 'offline'
}

/** Codex-style left rail: workspace switcher, projects, threads, footer. */
export function Sidebar({
  projects,
  sessions,
  project,
  session,
  onProject,
  onSession,
  onNewChat,
  className,
  connection,
  nodeLabel = 'fixture-node',
  nodeState = 'fixture',
}: Props) {
  return (
    <aside
      className={cn(
        'bg-sidebar text-sidebar-foreground flex h-full w-[264px] shrink-0 flex-col border-r',
        className,
      )}
    >
      <div className="flex items-center justify-between px-3 py-3">
        <button className="hover:bg-sidebar-accent flex items-center gap-2 rounded-md px-2 py-1.5 text-sm font-medium">
          <span className="bg-primary text-primary-foreground grid size-6 place-items-center rounded-md text-[11px] font-semibold">
            rp
          </span>
          rpiwork
          <ChevronDown className="text-muted-foreground size-3.5" />
        </button>
        <Button
          variant="ghost"
          size="icon"
          aria-label="搜索项目与会话"
          className="size-7"
        >
          <Search className="size-3.5" />
        </Button>
      </div>

      <div className="px-3">
        <Button
          onClick={onNewChat}
          className="h-8 w-full justify-start gap-2 text-[13px]"
          variant="secondary"
        >
          <Plus className="size-3.5" />
          新对话
          <kbd className="text-muted-foreground ml-auto text-[10px]">⌘J</kbd>
        </Button>
      </div>

      <ScrollArea className="mt-3 flex-1">
        <div className="flex flex-col gap-4 px-3 pb-4">
          {projects.map((item) => {
            const active = item.id === project
            const list = sessions
              .filter((entry) => entry.projectId === item.id)
              .sort((a, b) => b.updatedAt - a.updatedAt)
            return (
              <Collapsible key={item.id} defaultOpen>
                <CollapsibleTrigger className="hover:bg-sidebar-accent group flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[12px] font-medium">
                  <ChevronDown className="size-3.5 transition-transform group-data-[state=closed]:-rotate-90" />
                  {item.name}
                  <Badge variant="outline" className="ml-auto h-4 px-1 text-[10px] font-normal">
                    {list.length}
                  </Badge>
                </CollapsibleTrigger>
                <CollapsibleContent className="mt-1 flex flex-col gap-px pl-1">
                  {list.map((entry) => (
                    <button
                      key={entry.id}
                      onClick={() => onSession(entry.id)}
                      className={cn(
                        'hover:bg-sidebar-accent truncate rounded-md px-2 py-1.5 text-left text-[13px]',
                        active && entry.id === session
                          ? 'bg-sidebar-accent text-sidebar-accent-foreground font-medium'
                          : 'text-muted-foreground',
                      )}
                    >
                      {entry.title}
                    </button>
                  ))}
                </CollapsibleContent>
              </Collapsible>
            )
          })}
        </div>
      </ScrollArea>

      {connection ? <div className="px-3 pb-2">{connection}</div> : null}
      <Separator />
      <div className="flex items-center justify-between px-3 py-2 text-[11px] text-muted-foreground">
        <span className="flex items-center gap-1">
          <span
            className={cn(
              'size-1.5 rounded-full',
              nodeState === 'online' ? 'bg-emerald-500' : nodeState === 'offline' ? 'bg-destructive' : 'bg-muted-foreground/50',
            )}
          />
          {nodeLabel}
        </span>
        <Button variant="ghost" size="icon" aria-label="设置" className="size-6">
          <Settings className="size-3.5" />
        </Button>
      </div>
    </aside>
  )
}
