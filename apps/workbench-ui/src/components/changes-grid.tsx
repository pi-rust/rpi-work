import { useMemo } from 'react'
import { useTable } from '@tanstack/react-table'
import type { ColumnDef } from '@tanstack/react-table'

import { DataGrid, dataGridFeatures } from '@/components/reui/data-grid/data-grid'
import { DataGridTable } from '@/components/reui/data-grid/data-grid-table'
import { Badge } from '@/components/ui/badge'
import type { NodeChangedFile } from '@/lib/node'

type Features = typeof dataGridFeatures
type Props = { changes: NodeChangedFile[]; onOpen: (path: string) => void }

const STATUS_LABEL: Record<string, string> = { M: '改', A: '新', D: '删', R: '移', '?': '未跟踪' }

/** Worktree changes as a ReUI data grid (TanStack Table v9, features-first). */
export function ChangesGrid({ changes, onOpen }: Props) {
  const columns = useMemo<ColumnDef<Features, NodeChangedFile, unknown>[]>(
    () => [
      {
        id: 'path',
        accessorKey: 'path',
        header: '文件',
        cell: ({ row }) => <button type="button" className="cursor-pointer text-left font-mono text-[11px] [overflow-wrap:anywhere] focus-visible:outline-2 focus-visible:outline-offset-[-2px]" aria-label={`查看 ${row.original.path} 的 Diff`} onClick={event => { event.stopPropagation(); onOpen(row.original.path) }}>{row.original.path}</button>,
      },
      {
        id: 'status',
        accessorKey: 'status',
        header: '状态',
        size: 64,
        cell: ({ row }) => (
          <span className="text-muted-foreground font-mono text-[11px]">
            {STATUS_LABEL[row.original.status] ?? row.original.status}
          </span>
        ),
      },
      {
        id: 'staged',
        accessorKey: 'staged',
        header: '暂存',
        size: 76,
        cell: ({ row }) =>
          row.original.staged ? (
            <Badge variant="outline" className="h-4 px-1 text-[9px]">已暂存</Badge>
          ) : (
            <span className="text-muted-foreground text-[10px]">—</span>
          ),
      },
    ],
    [onOpen],
  )

  const table = useTable({ features: dataGridFeatures, columns, data: changes })

  return (
    <DataGrid table={table} recordCount={changes.length} onRowClick={(row) => onOpen(row.path)}>
      <DataGridTable />
    </DataGrid>
  )
}
