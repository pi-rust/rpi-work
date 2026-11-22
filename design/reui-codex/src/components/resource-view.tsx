import { useState } from 'react'
import { Boxes, FolderGit2, Package, Puzzle, ShieldCheck, Sparkles } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { cn } from '@/lib/utils'
import type { ResourceView } from '@/lib/viewmodel'

type Bundle = {
  manifest: { id: string; title: string; version: string }
  sources: Record<string, string>
  sha256: string
}
export type InstallState = {
  offers: unknown[]
  bundle: Bundle | null
  installing: boolean
  error: string | null
  onReview: (offer: unknown) => void
  onCancel: () => void
  onConfirm: (scope: 'global' | 'project') => void
}

type Props = {
  live: boolean
  resources: ResourceView
  project: string
  onRefresh: () => void
  install: InstallState
}

const KIND_ICON: Record<string, typeof Package> = {
  packages: Package,
  skills: Sparkles,
  extensions: Puzzle,
}

/** Resource center: installed native resources, grouped by the Rust catalog rules. */
export function ResourcesPage({ live, resources, project, onRefresh, install }: Props) {
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

      {install.bundle ? (
        <InstallReview
          bundle={install.bundle}
          installing={install.installing}
          error={install.error}
          onCancel={install.onCancel}
          onConfirm={install.onConfirm}
        />
      ) : null}

      {install.offers.length > 0 ? (
        <div className="flex flex-col gap-1 rounded-lg border border-dashed p-2">
          <span className="text-muted-foreground px-1 text-[11px]">可安装（公共库）</span>
          {install.offers.slice(0, 5).map((offer, index) => {
            const entry = offer as { manifest?: { title?: string; version?: string }; sha256?: string }
            return (
              <div key={index} className="flex items-center gap-2 px-1 py-0.5 text-[12px]">
                <Package className="text-muted-foreground size-3.5" />
                <span className="truncate">{entry.manifest?.title ?? entry.manifest?.version ?? '包'}</span>
                <span className="text-muted-foreground font-mono text-[11px]">{entry.manifest?.version}</span>
                <Button
                  variant="outline"
                  size="sm"
                  className="ml-auto h-6 px-2 text-[11px]"
                  aria-label={`审阅 ${entry.manifest?.title ?? '包'}`}
                  onClick={() => install.onReview(offer)}
                >
                  审阅
                </Button>
              </div>
            )
          })}
        </div>
      ) : null}

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

/** Review gate: nothing is acquired or installed until scope and trust are explicit. */
function InstallReview({
  bundle,
  installing,
  error,
  onCancel,
  onConfirm,
}: {
  bundle: Bundle
  installing: boolean
  error: string | null
  onCancel: () => void
  onConfirm: (scope: 'global' | 'project') => void
}) {
  const [scope, setScope] = useState<'global' | 'project'>('project')
  const [trusted, setTrusted] = useState(false)
  const files = Object.keys(bundle.sources ?? {})
  return (
    <div className="bg-card flex flex-col gap-3 rounded-lg border p-3 text-[12px]">
      <div className="flex items-center gap-2">
        <ShieldCheck className="text-muted-foreground size-4" />
        <span className="font-medium">{bundle.manifest.title || bundle.manifest.id}</span>
        <Badge variant="outline" className="h-5 px-1.5 text-[10px] font-normal">
          {bundle.manifest.version}
        </Badge>
        <span className="text-muted-foreground ml-auto font-mono text-[10px]">
          {bundle.sha256.slice(0, 12)}…
        </span>
      </div>
      <Separator />
      <div className="text-muted-foreground flex flex-col gap-0.5 font-mono text-[11px]">
        {files.slice(0, 6).map((path) => (
          <span key={path}>· {path}</span>
        ))}
        {files.length > 6 ? <span>· 还有 {files.length - 6} 个文件</span> : null}
      </div>
      <div className="flex items-center gap-3">
        <label className="text-muted-foreground flex items-center gap-1">
          范围
          <select
            aria-label="安装范围"
            value={scope}
            onChange={(event) => setScope(event.target.value as 'global' | 'project')}
            className="border-input bg-background h-6 rounded border px-1 text-[11px]"
          >
            <option value="project">项目</option>
            <option value="global">全局</option>
          </select>
        </label>
        <label className="flex items-center gap-1 text-[11px]">
          <input
            type="checkbox"
            checked={trusted}
            onChange={(event) => setTrusted(event.target.checked)}
            aria-label="我已审阅来源"
          />
          我已审阅来源与文件清单
        </label>
        <Button variant="ghost" size="sm" className="ml-auto h-6 px-2 text-[11px]" onClick={onCancel}>
          取消
        </Button>
        <Button
          size="sm"
          className="h-6 px-2 text-[11px]"
          disabled={!trusted || installing}
          onClick={() => onConfirm(scope)}
        >
          {installing ? '安装中…' : '确认安装'}
        </Button>
      </div>
      {error ? <p className="text-destructive text-[11px]">{error}</p> : null}
    </div>
  )
}
