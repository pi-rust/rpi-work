import { useRef, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { useTree } from '@headless-tree/react'
import {
  asyncDataLoaderFeature,
  hotkeysCoreFeature,
  selectionFeature,
} from '@headless-tree/core'

import { Tree, TreeItem, TreeItemLabel } from '@/components/reui/tree'
import { ChangesGrid } from '@/components/changes-grid'
import { FileViewer } from '@/components/file-viewer'
import type { FileRead } from '@/lib/workspace'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { NodeChangedFile, NodeFileEntry } from '@/lib/node'

type Props = {
  live: boolean
  presentationReady: boolean
  changes: NodeChangedFile[]
  fileRead: FileRead | null
  onRefresh: () => void
  onOpen: (path: string, staged?: boolean) => void
  onOpenSource: (path: string) => void
  listDir: (path: string) => Promise<NodeFileEntry[]>
}

/** Mount the tree/viewer only after its shared display engine is initialized. */
export function FilePanel(props: Props) {
  if (!props.live) return <p className="text-muted-foreground p-3 text-[12px]">文件与 Diff 需要连接真实节点；当前没有工作区。</p>
  if (!props.presentationReady) return <p role="status" className="text-muted-foreground p-3 text-[12px]">正在加载文件展示…</p>
  return <ReadyFilePanel {...props} />
}

/** Right-rail file panel: changed files from git, plus the selected diff or source. */
function ReadyFilePanel({
  changes,
  fileRead,
  onRefresh,
  onOpen,
  onOpenSource,
  listDir,
}: Props) {
  const [staged, setStaged] = useState(() => fileRead?.mode === 'diff' && fileRead.staged)
  const [tab, setTab] = useState<'changes' | 'tree'>('changes')
  // Node directory entries are cached by path; headless-tree asks lazily.
  const entries = useRef(new Map<string, NodeFileEntry>())
  // A file that is also changed should open its diff, not its saved source.
  const changed = new Set(changes.map((change) => change.path))
  const openPath = (path: string) => changed.has(path) ? onOpen(path, staged) : onOpenSource(path)
  const tree = useTree<NodeFileEntry>({
    rootItemId: '',
    onPrimaryAction: item => { if (!item.isFolder()) openPath(item.getId()) },
    getItemName: (item) => item.getItemData()?.name ?? '',
    isItemFolder: (item) => item.getItemData()?.kind === 'directory',
    dataLoader: {
      getItem: (itemId) =>
        entries.current.get(itemId) ?? {
          name: itemId === '' ? '' : itemId,
          path: itemId,
          kind: itemId === '' ? 'directory' : 'file',
          size: 0,
        },
      getChildren: async (itemId) => {
        const children = await listDir(itemId)
        for (const entry of children) entries.current.set(entry.path, entry)
        return children.map((entry) => entry.path)
      },
    },
    features: [asyncDataLoaderFeature, selectionFeature, hotkeysCoreFeature],
  })

  return (
    <div className="wb-file-panel flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-3 pt-2 text-[11px]">
        <div className="flex items-center gap-1">
          <button
            data-testid="tab-changes"
            onClick={() => setTab('changes')}
            className={cn('rounded px-1.5 py-0.5', tab === 'changes' ? 'bg-muted text-foreground' : 'text-muted-foreground')}
          >
            改动 · {changes.length}
          </button>
          <button
            data-testid="tab-tree"
            onClick={() => {
              setTab('tree')
              tree.getItemInstance?.('')?.expand()
            }}
            className={cn('rounded px-1.5 py-0.5', tab === 'tree' ? 'bg-muted text-foreground' : 'text-muted-foreground')}
          >
            目录
          </button>
        </div>
        <div className="flex items-center gap-1">
          <label className="text-muted-foreground flex items-center gap-1">
            <input
              type="checkbox"
              checked={staged}
              onChange={(event) => {
                setStaged(event.target.checked)
                if (fileRead?.mode === 'diff') onOpen(fileRead.path, event.target.checked)
              }}
              aria-label="仅看已暂存"
              data-testid="staged-toggle"
            />
            暂存
          </label>
          <Button variant="ghost" size="icon" className="size-6" aria-label="刷新改动" onClick={onRefresh}>
            <RefreshCw className="size-3.5" />
          </Button>
        </div>
      </div>
      <div className="max-h-48 shrink-0 overflow-y-auto px-2 py-1">
        {tab === 'tree' ? (
          <Tree tree={tree} className="py-1 text-[11px]">
            {tree.getItems().map((item) => (
              <TreeItem
                key={item.getId()}
                item={item}
                className={cn(fileRead?.path === item.getId() && 'bg-muted/70')}
              >
                <TreeItemLabel
                  className="w-full font-mono"
                  onClick={(event) => {
                    event.stopPropagation()
                    if (item.isFolder()) {
                      if (item.isExpanded()) item.collapse()
                      else item.expand()
                    } else {
                      openPath(item.getId())
                    }
                  }}
                >
                  {item.getItemName()}
                </TreeItemLabel>
              </TreeItem>
            ))}
          </Tree>
        ) : changes.length === 0 ? (
          <p className="text-muted-foreground px-1 py-2 text-[11px]">没有未提交的改动。</p>
        ) : (
          <ChangesGrid changes={changes} onOpen={(path) => onOpen(path, staged)} />
        )}
      </div>
      {fileRead === null ? <p className="text-muted-foreground border-t p-3 text-[12px]">选择一个文件查看源码或原始 Diff。</p> : (
        <FileViewer key={JSON.stringify([fileRead.path, fileRead.mode, fileRead.staged])} file={fileRead}
          retry={() => fileRead.mode === 'diff' ? onOpen(fileRead.path, fileRead.staged) : onOpenSource(fileRead.path)} />
      )}
    </div>
  )
}
