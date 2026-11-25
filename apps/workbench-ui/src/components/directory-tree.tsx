// TanStack Virtual updates its instance in place, as in the virtual changes grid.
'use no memo'

import { useCallback, useLayoutEffect, useState, type RefObject } from 'react'
import type { TreeInstance } from '@headless-tree/core'
import { defaultRangeExtractor, useVirtualizer, type Range } from '@tanstack/react-virtual'
import { Tree, TreeItem, TreeItemLabel } from '@/components/reui/tree'
import type { NodeFileEntry } from '@/lib/node'
import { cn } from '@/lib/utils'
import { useIsMobile } from '@/lib/viewport'

type Props = {
  tree: TreeInstance<NodeFileEntry>
  viewport: RefObject<HTMLDivElement | null>
  notices: RefObject<HTMLDivElement | null>
  selectedPath?: string
}

const WINDOW_THRESHOLD = 80

/** Window the DOM while Headless Tree keeps the complete order and sibling metadata. */
export function DirectoryTree({ tree, viewport, notices, selectedPath }: Props) {
  const items = tree.getItems()
  const windowed = items.length > WINDOW_THRESHOLD
  const rowHeight = useIsMobile() ? 46 : 34
  const [scrollMargin, setScrollMargin] = useState(0)
  const [domFocus, setDomFocus] = useState<string | null>(null)
  const focusedIndex = tree.getFocusedItem()?.getItemMeta().index ?? -1
  const domFocusIndex = domFocus === null ? -1 : tree.getItemInstance(domFocus).getItemMeta().index
  const getScrollElement = useCallback(() => viewport.current, [viewport])
  const getItemKey = useCallback((index: number) => items[index].getId(), [items])
  const rangeExtractor = useCallback((range: Range) => {
    const indices = new Set(defaultRangeExtractor(range))
    // Keep both the actual focused row and the next tab stop during keyboard jumps.
    for (const index of [domFocusIndex, focusedIndex]) {
      if (index >= 0 && index < range.count) indices.add(index)
    }
    return [...indices].sort((a, b) => a - b)
  }, [domFocusIndex, focusedIndex])
  const virtualizer = useVirtualizer({
    count: items.length,
    enabled: windowed,
    getScrollElement,
    getItemKey,
    estimateSize: () => rowHeight,
    overscan: 5,
    paddingStart: 4,
    paddingEnd: 4,
    scrollMargin,
    rangeExtractor,
  })

  useLayoutEffect(() => {
    if (!windowed) return
    virtualizer.measure()
    const focused = tree.getFocusedItem()?.getElement()
    if (focused && focused === document.activeElement) focused.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [rowHeight, tree, virtualizer, windowed])
  useLayoutEffect(() => {
    const scrollElement = viewport.current
    const treeElement = tree.getElement()
    const noticeElement = notices.current
    if (!windowed || !scrollElement || !treeElement || !noticeElement) return
    const measure = () => {
      if (!scrollElement.clientHeight) return
      setScrollMargin(treeElement.getBoundingClientRect().top - scrollElement.getBoundingClientRect().top + scrollElement.scrollTop)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(scrollElement)
    observer.observe(noticeElement)
    return () => observer.disconnect()
  }, [notices, tree, viewport, windowed])

  const renderRow = (index: number, start?: number) => {
    const item = items[index]
    return <TreeItem
      key={item.getId()}
      item={item}
      className={cn(start !== undefined && 'wb-directory-row', selectedPath === item.getId() && 'bg-muted/70')}
      style={start === undefined ? undefined : { height: rowHeight, transform: `translateY(${start - scrollMargin}px)` }}
      onFocus={() => {
        if (!item.isFocused()) item.setFocused()
        setDomFocus(item.getId())
      }}
      onBlur={() => setDomFocus(null)}
    >
      <TreeItemLabel
        className="w-full font-mono"
        onClick={event => {
          event.stopPropagation()
          if (!item.isFolder()) item.primaryAction()
          else if (item.isExpanded()) item.collapse()
          else item.expand()
        }}
      >
        {windowed ? <span className="min-w-0 truncate" title={item.getItemName()}>{item.getItemName()}</span> : item.getItemName()}
      </TreeItemLabel>
    </TreeItem>
  }

  return <Tree tree={tree}
    className={cn('text-[11px]', windowed ? 'wb-directory-window relative' : 'py-1')}
    style={windowed ? { height: virtualizer.getTotalSize() } : undefined}
  >
    {windowed ? virtualizer.getVirtualItems().map(row => renderRow(row.index, row.start)) : items.map((_, index) => renderRow(index))}
  </Tree>
}
