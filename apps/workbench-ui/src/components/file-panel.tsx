import { useEffectEvent, useLayoutEffect, useRef, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { useTree } from '@headless-tree/react'
import {
  hotkeysCoreFeature,
  selectionFeature,
  syncDataLoaderFeature,
} from '@headless-tree/core'

import { DirectoryTree } from '@/components/directory-tree'
import { ChangesGrid } from '@/components/changes-grid'
import { FileViewer } from '@/components/file-viewer'
import type { FileRead, Workspace } from '@/lib/workspace'
import type { ReviewDelivery } from '@/lib/review-delivery'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { NodeChangedFile, NodeFileEntry, DirectoryPage } from '@/lib/node'

type DirectoryRead = {
  entries?: NodeFileEntry[]
  cursor?: string
} & (
  | { status: 'ready' | 'changed' }
  | { status: 'loading'; more: boolean }
  | { status: 'error'; more: boolean; error: string }
)

type Props = {
  live: boolean
  visible: boolean
  presentationReady: boolean
  changes: NodeChangedFile[]
  review?: Workspace['changeReview'] & { disabled: boolean; delivery: ReviewDelivery; run: () => void }
  fileRead: FileRead | null
  onRefresh: () => void
  onOpen: (path: string, staged?: boolean) => void
  onOpenSource: (path: string) => void
  feedback?: React.ComponentProps<typeof FileViewer>['feedback']
  expandedDirectories: string[]
  onExpandedDirectoriesChange: (paths: string[]) => void
  listDir: (path: string, cursor?: string) => Promise<DirectoryPage | null>
}

/** Mount the tree/viewer only after its shared display engine is initialized. */
export function FilePanel(props: Props) {
  if (!props.live) return <p className="text-muted-foreground p-3 text-[12px]">文件与 Diff 需要连接真实节点；当前没有工作区。</p>
  if (!props.presentationReady) return <p role="status" className="text-muted-foreground p-3 text-[12px]">正在加载文件展示…</p>
  return <ReadyFilePanel {...props} />
}

/** Right-rail file panel: changed files from git, plus the selected diff or source. */
function ReadyFilePanel({
  visible,
  changes,
  review,
  fileRead,
  onRefresh,
  onOpen,
  onOpenSource,
  feedback,
  expandedDirectories,
  onExpandedDirectoriesChange,
  listDir,
}: Props) {
  const [staged, setStaged] = useState(() => fileRead?.mode === 'diff' && fileRead.staged)
  const [tab, setTab] = useState<'changes' | 'tree'>('changes')
  const treeActive = useRef(false)
  const viewport = useRef<HTMLDivElement>(null)
  const refreshButton = useRef<HTMLButtonElement>(null)
  const noticeContainer = useRef<HTMLDivElement>(null)
  // One listing cache for both the lazy tree and explicit GETs. The sync loader
  // reads it without scheduling network requests when the tree is rebuilt.
  const entries = useRef(new Map<string, NodeFileEntry>())
  const reads = useRef(new Map<string, DirectoryRead>())
  const [directoryReads, setDirectoryReads] = useState(() => new Map<string, DirectoryRead>())
  const requests = useRef(new Map<string, symbol>())
  const refreshInProgress = useRef(false)
  const [refreshing, setRefreshing] = useState(false)
  const mounted = useRef(false)
  useLayoutEffect(() => {
    mounted.current = true
    return () => { mounted.current = false }
  }, [])
  // A file that is also changed should open its diff, not its saved source.
  const changed = new Set(changes.map((change) => change.path))
  const openPath = (path: string) => changed.has(path) ? onOpen(path, staged) : onOpenSource(path)

  const publishReads = () => setDirectoryReads(new Map(reads.current))
  const readDirectory = async (path: string, more = false, trigger?: HTMLButtonElement): Promise<boolean> => {
    if (!mounted.current || requests.current.has(path)) return false
    const previousRead = reads.current.get(path)
    const previous = previousRead?.entries
    const previousCursor = previousRead?.cursor
    if (more && !previousCursor) return false
    const request = Symbol(path)
    requests.current.set(path, request)
    const current = () => mounted.current && requests.current.get(path) === request
    reads.current.set(path, { status: 'loading', entries: previous, cursor: previousCursor, more })
    publishReads()
    try {
      const children = more ? [...(previous ?? [])] : []
      const next = new Map(children.map(entry => [entry.path, entry]))
      const prefix = path ? `${path}/` : ''
      const directChild = (id: string) => id.startsWith(prefix) ? id.slice(prefix.length).split('/', 1)[0] : ''
      const expandedChildren = new Set(tree.getState().expandedItems.map(directChild).filter(Boolean))
      const seenCursors = new Set<string>()
      let cursor = more ? previousCursor : undefined
      do {
        if (cursor) {
          if (seenCursors.has(cursor)) throw new Error('目录分页返回了重复的游标')
          seenCursors.add(cursor)
        }
        const page = await listDir(path, cursor)
        if (!current()) return false
        if (!page) {
          if (previousRead) reads.current.set(path, previousRead)
          else reads.current.delete(path)
          publishReads()
          return false
        }
        if (page.status === 'changed') {
          reads.current.set(path, { status: 'changed', entries: previous })
          if (trigger && document.activeElement === trigger) refreshButton.current?.focus()
          publishReads()
          return false
        }
        for (const entry of page.listing.entries) {
          if (next.has(entry.path)) throw new Error('目录分页返回了重复的路径')
          next.set(entry.path, entry)
          children.push(entry)
          expandedChildren.delete(entry.name)
        }
        cursor = page.listing.pagination?.nextCursor
        // A refresh reloads the visible range. A saved expansion may need later pages.
      } while (cursor && !more && (children.length < (previous?.length ?? 0) || expandedChildren.size > 0))
      const detached = new Set<string>()
      const forget = (id: string) => {
        if (detached.has(id)) return
        detached.add(id)
        const cached = reads.current.get(id)?.entries
        entries.current.delete(id)
        reads.current.delete(id)
        requests.current.delete(id)
        cached?.forEach(entry => forget(entry.path))
      }
      for (const entry of previous ?? []) {
        if (next.get(entry.path)?.kind !== entry.kind) forget(entry.path)
      }
      for (const entry of children) entries.current.set(entry.path, entry)
      reads.current.set(path, { status: 'ready', entries: children, cursor })
      // Only completion disproves a saved branch whose first component is absent.
      const expanded = tree.getState().expandedItems
      const surviving = expanded.filter(id => {
        const child = directChild(id)
        if (!child) return true
        const entry = next.get(prefix + child)
        return entry ? entry.kind === 'directory' : cursor !== undefined
      })
      if (expanded.length !== surviving.length) tree.applySubStateUpdate('expandedItems', surviving)
      if (detached.size) {
        tree.applySubStateUpdate('selectedItems', ids => ids.filter(id => !detached.has(id)))
        if (detached.has(tree.getState().focusedItem ?? '')) tree.applySubStateUpdate('focusedItem', null)
      }
      tree.rebuildTree()
      if (trigger && document.activeElement === trigger && (!cursor || !more)) refreshButton.current?.focus()
      publishReads()
      const parentVisible = path === '' || (tree.getItemInstance(path).getItemMeta().index >= 0 && surviving.includes(path))
      if (treeActive.current && parentVisible) loadExpanded(children.map(entry => entry.path))
      return true
    } catch (cause) {
      if (current()) {
        reads.current.set(path, { status: 'error', entries: previous, cursor: previousCursor, more, error: String(cause) })
        publishReads()
      }
      return false
    } finally {
      if (current()) requests.current.delete(path)
    }
  }

  const loadExpanded = (visibleIds: string[]) => {
    if (!treeActive.current) return
    const expanded = new Set(tree.getState().expandedItems)
    for (const id of visibleIds) {
      if (expanded.has(id) && entries.current.get(id)?.kind === 'directory' && !reads.current.has(id)) void readDirectory(id)
    }
  }

  const tree = useTree<NodeFileEntry>({
    rootItemId: '',
    initialState: { expandedItems: expandedDirectories },
    onPrimaryAction: item => { if (!item.isFolder()) openPath(item.getId()) },
    getItemName: (item) => item.getItemData()?.name ?? '',
    isItemFolder: (item) => item.getItemData()?.kind === 'directory',
    // The virtual range retains the focused row, so keyboard jumps can use its DOM position.
    scrollToItem: item => item.getElement()?.scrollIntoView({ block: 'nearest', inline: 'nearest' }),
    setExpandedItems: () => {
      if (!mounted.current) return
      onExpandedDirectoriesChange(tree.getState().expandedItems)
      // The state has changed, but the cached item metadata has not been rebuilt yet.
      loadExpanded(tree.getItemsMeta().map(item => item.itemId))
    },
    dataLoader: {
      getItem: id => entries.current.get(id) ?? { name: id, path: id, kind: id === '' ? 'directory' : 'file', size: 0 },
      getChildren: id => reads.current.get(id)?.entries?.map(entry => entry.path) ?? [],
    },
    features: [syncDataLoaderFeature, selectionFeature, hotkeysCoreFeature],
  })
  const items = tree.getItems()
  const visibleIds = new Set(items.map(item => item.getId()))
  const notices = [...directoryReads].filter(([path, read]) =>
    (path === '' || (visibleIds.has(path) && tree.getItemInstance(path).isExpanded())) &&
    (read.status !== 'ready' || read.cursor || read.entries?.length === 0))

  const loadVisibleTree = useEffectEvent(() => {
    if (!reads.current.has('')) void readDirectory('')
    else loadExpanded(tree.getItems().map(item => item.getId()))
  })
  useLayoutEffect(() => {
    treeActive.current = visible && tab === 'tree'
    if (treeActive.current) loadVisibleTree()
    return () => { treeActive.current = false }
  }, [visible, tab])

  const refreshDirectory = async () => {
    if (!mounted.current || refreshInProgress.current || requests.current.has('')) return
    refreshInProgress.current = true
    setRefreshing(true)
    // Snapshot only the visible, expanded folders. Walk parent-first so a
    // deleted, retyped or failed branch never triggers descendant GETs.
    const open = tree.getItems().filter(item => item.isFolder() && item.isExpanded())
      .map(item => ({ path: item.getId(), parent: item.getItemMeta().parentId }))
    const refreshed = new Set<string>()
    try {
      if (await readDirectory('')) refreshed.add('')
      for (const { path, parent } of open) {
        if (!mounted.current) break
        const item = tree.getItemInstance(path)
        if (!refreshed.has(parent) || item.getItemMeta().index < 0 || !item.isFolder() || !item.isExpanded()) continue
        // Refresh supersedes any earlier read for this surviving directory.
        requests.current.delete(path)
        if (await readDirectory(path)) refreshed.add(path)
      }
    } finally {
      refreshInProgress.current = false
      if (mounted.current) setRefreshing(false)
    }
  }

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
            onClick={() => setTab('tree')}
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
          <Button ref={refreshButton} variant="ghost" size="icon" className="wb-file-refresh size-6"
            aria-label={tab === 'tree' ? '刷新目录' : '刷新改动'}
            title={tab === 'tree' ? '刷新根目录和已展开的目录' : '刷新改动'}
            aria-disabled={tab === 'tree' && (refreshing || directoryReads.get('')?.status === 'loading') ? true : undefined}
            onClick={tab === 'tree' ? () => void refreshDirectory() : onRefresh}>
            <RefreshCw className="size-3.5" aria-hidden="true" />
          </Button>
        </div>
      </div>
      {review && tab === 'changes' ? <div className="flex flex-wrap items-center gap-x-2 px-3 text-[11px]">
        <Button type="button" variant="outline" size="sm" className="h-7 aria-disabled:opacity-50" aria-disabled={review.disabled || review.pending || changes.length === 0}
          onClick={() => { if (!review.disabled && !review.pending && changes.length) review.run() }}>运行审阅</Button>
        <span className="text-muted-foreground">{review.delivery === 'detached' ? '独立会话' : '当前会话'}</span>
        {review.pending ? <p role="status" className="basis-full py-1">正在提交审阅请求…</p> : review.notice ? <p role="status" className="basis-full break-words py-1">{review.notice}</p> : null}
      </div> : null}
      <div ref={viewport} className="wb-directory-viewport max-h-48 shrink-0 overflow-y-auto px-2 py-1">
        {tab === 'tree' ? (
          <>
          <div ref={noticeContainer}>
          {notices.map(([path, read]) => {
            const label = path ? `目录 ${path}` : '项目根目录'
            return <div key={path} role="group" aria-label={label} className="wb-directory-notice">
              {read.status === 'changed' ? (
                <p role="alert">{label}内容已变化，请刷新目录。</p>
              ) : read.status === 'error' ? (
                <p role="alert">{label}{read.more ? '后续页读取失败' : read.entries ? '刷新失败' : '读取失败'}：{read.error}{read.entries ? '（保留上次读取结果）' : ''}</p>
              ) : read.status === 'loading' ? (
                <p role="status">{read.more ? `正在加载${label}的后续文件…` : read.entries ? `正在刷新${label}…（保留上次读取结果）` : `正在读取${label}…`}</p>
              ) : read.entries?.length === 0 ? <p role="status">{label}{path ? ' ' : ''}为空。</p> : null}
              {read.cursor && (read.status === 'ready' || ('more' in read && read.more)) ? (
                <Button variant="outline" size="sm" aria-disabled={read.status === 'loading' || refreshing}
                  aria-label={`${read.status === 'error' ? '重试加载' : '加载更多'}${label}文件`}
                  onClick={event => { if (!refreshInProgress.current) void readDirectory(path, true, event.currentTarget) }}>
                  {read.status === 'error' ? '重试加载' : '加载更多'}{label}文件
                </Button>
              ) : read.status === 'changed' ? (
                <Button variant="outline" size="sm" onClick={() => void refreshDirectory()}>刷新{label}</Button>
              ) : read.status === 'error' ? (
                <Button variant="outline" size="sm" onClick={() => void readDirectory(path)}>重试读取{label}</Button>
              ) : null}
            </div>
          })}
          </div>
          <DirectoryTree tree={tree} viewport={viewport} notices={noticeContainer} selectedPath={fileRead?.path} />
          </>
        ) : changes.length === 0 ? (
          <p className="text-muted-foreground px-1 py-2 text-[11px]">没有未提交的改动。</p>
        ) : (
          <ChangesGrid changes={changes} onOpen={(path) => onOpen(path, staged)} />
        )}
      </div>
      {fileRead === null ? <p className="text-muted-foreground border-t p-3 text-[12px]">选择一个文件查看源码或原始 Diff。</p> : (
        <FileViewer key={JSON.stringify([fileRead.path, fileRead.mode, fileRead.staged])} file={fileRead} feedback={feedback}
          retry={() => fileRead.mode === 'diff' ? onOpen(fileRead.path, fileRead.staged) : onOpenSource(fileRead.path)} />
      )}
    </div>
  )
}
