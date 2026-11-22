import { useRef, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { useTree } from '@headless-tree/react'
import {
  asyncDataLoaderFeature,
  hotkeysCoreFeature,
  selectionFeature,
} from '@headless-tree/core'

import { Tree, TreeItem, TreeItemLabel } from '@/components/reui/tree'

import {
  CodeBlock,
  CodeBlockCopyButton,
  CodeBlockHeader,
  CodeBlockTitle,
} from '@/components/reui/code-block/code-block'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { NodeChangedFile, NodeFileEntry } from '@/lib/node'

type Props = {
  live: boolean
  changes: NodeChangedFile[]
  filePath: string | null
  fileDiff: string | null
  fileBody: string | null
  onRefresh: () => void
  onOpen: (path: string, staged?: boolean) => void
  onOpenSource: (path: string) => void
  listDir: (path: string) => Promise<NodeFileEntry[]>
}

const STATUS_LABEL: Record<string, string> = {
  M: '改',
  A: '新',
  D: '删',
  R: '移',
  '?': '未跟踪',
}

function languageFor(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase() ?? ''
  return (
    {
      rs: 'rust',
      ts: 'typescript',
      tsx: 'tsx',
      js: 'javascript',
      json: 'json',
      py: 'python',
      sh: 'shell',
      css: 'css',
      html: 'html',
      md: 'markdown',
      toml: 'toml',
      yaml: 'yaml',
      yml: 'yaml',
    }[extension] ?? 'text'
  )
}

/** Right-rail file panel: changed files from git, plus the selected diff or source. */
export function FilePanel({
  live,
  changes,
  filePath,
  fileDiff,
  fileBody,
  onRefresh,
  onOpen,
  onOpenSource,
  listDir,
}: Props) {
  const [staged, setStaged] = useState(false)
  const [tab, setTab] = useState<'changes' | 'tree'>('changes')
  // Node directory entries are cached by path; headless-tree asks lazily.
  const entries = useRef(new Map<string, NodeFileEntry>())
  const tree = useTree<NodeFileEntry>({
    rootItemId: '',
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

  if (!live) {
    return (
      <p className="text-muted-foreground p-3 text-[12px]">
        文件与 Diff 需要连接节点（fixture 模式没有工作区）。
      </p>
    )
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
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
                if (filePath) onOpen(filePath, event.target.checked)
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
                className={cn(filePath === item.getId() && 'bg-muted/70')}
              >
                <TreeItemLabel
                  className="w-full font-mono"
                  onClick={(event) => {
                    event.stopPropagation()
                    if (item.isFolder()) {
                      if (item.isExpanded()) item.collapse()
                      else item.expand()
                    } else {
                      onOpenSource(item.getId())
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
          changes.slice(0, 60).map((change) => (
            <button
              key={`${change.path}-${change.staged ? 's' : 'w'}`}
              onClick={() => onOpen(change.path)}
              className={cn(
                'hover:bg-muted/60 flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-[11px]',
                filePath === change.path && 'bg-muted/70',
              )}
              title={change.path}
            >
              <span className="text-muted-foreground w-3 shrink-0 font-mono">
                {STATUS_LABEL[change.status] ?? change.status}
              </span>
              <span className="truncate font-mono">{change.path}</span>
              {change.staged ? (
                <Badge variant="outline" className="ml-auto h-4 px-1 text-[9px]">
                  已暂存
                </Badge>
              ) : null}
            </button>
          ))
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto border-t p-2">
        {filePath === null ? (
          <p className="text-muted-foreground text-[11px]">选择一个改动查看 Diff。</p>
        ) : (
          <CodeBlock
            code={fileDiff ?? fileBody ?? ''}
            language={fileDiff ? 'diff' : languageFor(filePath)}
            showLineNumbers={!fileDiff}
            wrap={Boolean(fileDiff)}
            themes={{ light: 'css-variables', dark: 'css-variables' }}
          >
            <CodeBlockHeader>
              <CodeBlockTitle>{filePath}</CodeBlockTitle>
              <span className="ml-auto" />
              <CodeBlockCopyButton />
            </CodeBlockHeader>
          </CodeBlock>
        )}
      </div>
    </div>
  )
}
