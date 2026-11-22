import { Boxes, FolderGit2, Package, Puzzle, Sparkles } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { ResourceView } from '@/lib/viewmodel'

type Props = {
  live: boolean
  resources: ResourceView
  project: string
  onRefresh: () => void
}

const KIND_ICON: Record<string, typeof Package> = {
  packages: Package,
  skills: Sparkles,
  extensions: Puzzle,
}

/** Resource center: installed native resources, grouped by the Rust catalog rules. */
export function ResourcesPage({ live, resources, project, onRefresh }: Props) {
  if (!live) {
    return (
      <Empty
        title="资源中心需要连接节点"
        body="fixture 模式没有工作区，也就没有 packages / skills / extensions。"
      />
    )
  }
  const [packages, skills, extensions] = resources.counts
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 py-6">
      <div className="flex items-center gap-3">
        <Boxes className="text-muted-foreground size-4" />
        <span className="text-[15px] font-medium">资源中心</span>
        <span className="text-muted-foreground text-[12px]">{project}</span>
        <Button variant="outline" size="sm" className="ml-auto h-7 text-[12px]" onClick={onRefresh}>
          刷新
        </Button>
      </div>

      <div className="grid grid-cols-3 gap-2">
        <Stat label="Packages" value={packages} kind="packages" />
        <Stat label="Skills" value={skills} kind="skills" />
        <Stat label="Extensions" value={extensions} kind="extensions" />
      </div>

      {resources.rows.length === 0 ? (
        <Empty title="没有已安装的资源" body="节点目录为空；安装流程属于 P7b。" />
      ) : (
        <div className="overflow-hidden rounded-lg border">
          <div className="bg-muted/40 text-muted-foreground grid grid-cols-[1fr_70px_70px_90px] gap-2 px-3 py-1.5 text-[11px]">
            <span>名称</span>
            <span>类型</span>
            <span>版本</span>
            <span>范围</span>
          </div>
          {resources.rows.map((row) => {
            const Icon = KIND_ICON[row.kind] ?? Package
            return (
              <div
                key={row.reference}
                className="hover:bg-muted/30 grid grid-cols-[1fr_70px_70px_90px] items-center gap-2 border-t px-3 py-2 text-[12px]"
                title={`${row.reference} · ${row.origin}`}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <Icon className="text-muted-foreground size-3.5 shrink-0" />
                  <span className="truncate">{row.name}</span>
                </span>
                <span className="text-muted-foreground text-[11px]">{row.kindLabel}</span>
                <span className="text-muted-foreground font-mono text-[11px]">{row.version}</span>
                <Badge variant="outline" className="h-5 w-fit gap-1 px-1.5 text-[10px] font-normal">
                  <FolderGit2 className="size-3" />
                  {row.scope === 'global' ? '全局' : '项目'}
                </Badge>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

function Stat({ label, value, kind }: { label: string; value: number; kind: string }) {
  const Icon = KIND_ICON[kind] ?? Package
  return (
    <div className={cn('bg-card flex items-center gap-3 rounded-lg border p-3')}>
      <Icon className="text-muted-foreground size-4" />
      <span className="text-muted-foreground text-[11px]">{label}</span>
      <span className="ml-auto font-mono text-[13px] tabular-nums">{value}</span>
    </div>
  )
}

function Empty({ title, body }: { title: string; body: string }) {
  return (
    <div className="text-muted-foreground mx-auto flex max-w-md flex-col items-center gap-2 py-24 text-center text-[13px]">
      <span className="text-foreground text-[15px] font-medium">{title}</span>
      <span>{body}</span>
    </div>
  )
}
