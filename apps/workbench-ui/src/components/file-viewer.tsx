import { useMemo, type CSSProperties } from 'react'
import { CopyTextButton } from '@/components/copy-text-button'
import { diffView, fileView, type DiffView } from '@/lib/viewmodel'
import type { FileRead } from '@/lib/workspace'
import './file-viewer.css'

/** A read-only projection, not an editor, review machine or git action owner. */
const fallbackLabels: Record<NonNullable<DiffView['fallback_reason']>, string> = {
  limit: '内容较大，按完整原文显示；行号与高亮已停用。',
  invalid: '此 Diff 未通过统一差异校验，按完整原文显示；不提供行号。',
  unsupported: '此 Diff 的格式暂不支持编号，按完整原文显示。',
  no_hunks: '此 Diff 没有可编号的 hunk，按完整原文显示。',
}
export function FileViewer({ file, retry }: { file: FileRead; retry: () => void }) {
  const content = file.status === 'ready' ? file.content : ''
  const diff = file.mode === 'diff'
  const view = useMemo(() => diff ? diffView(content, file.path) : null, [content, file.path, diff])
  const rows = useMemo(() => view?.rows ?? fileView(content, file.path, false), [view, content, file.path])
  return <section className="wb-file-view" aria-label="文件读取" aria-busy={file.status === 'loading'}>
    <div className="wb-file-header">
      <div><h3>{file.path}</h3><span>{diff ? `${view?.structured ? '统一 Diff' : '原始 Diff'} · ${file.staged ? '已暂存' : '工作区'}` : '只读源码'}</span></div>
      {file.status === 'ready' ? <CopyTextButton text={content} label={diff ? '复制 Diff' : '复制源码'} /> : null}
    </div>
    {file.status === 'loading' ? <p className="wb-file-notice" role="status">正在读取 {file.path}…</p>
      : file.status === 'error' ? <div className="wb-file-notice" role="alert"><p>读取失败：{file.error}</p><button type="button" onClick={retry}>重试读取文件</button></div>
      : content === '' ? <p className="wb-file-notice">{diff ? '当前范围没有 Diff。' : '空文件'}</p>
      : <div className="wb-source-code" tabIndex={0} role="region" aria-label="只读文件内容"
          style={view?.structured ? { '--wb-diff-gutter': `max(40px, calc(${view.gutter_digits}ch + 14px))` } as CSSProperties : undefined}>
        {rows.map((row, index) => {
          const old = 'old_number' in row ? row.old_number : null
          const next = 'new_number' in row ? row.new_number : null
          const numbered = view?.structured && (old !== null || next !== null)
          return <div key={index} className={`wb-source-line${numbered ? ' wb-diff-row' : ''}`} data-kind={row.kind}>
            {row.number !== null ? <span className="wb-source-number" aria-hidden="true">{row.number}</span> : null}
            {numbered ? <>
              <span className="wb-diff-number wb-diff-old" aria-hidden="true">{old}</span>
              <span className="wb-diff-number wb-diff-new" aria-hidden="true">{next}</span>
              <span className="sr-only">{old !== null ? `旧行 ${old}。` : ''}{next !== null ? `新行 ${next}。` : ''}</span>
            </> : null}
            <code>{row.tokens.map((token, tokenIndex) => <span key={tokenIndex} className={token.class || undefined}>{token.text}</span>)}</code>
          </div>
        })}
      </div>}
    <div className="wb-file-status"><span>{view?.fallback_reason ? fallbackLabels[view.fallback_reason] : rows[0]?.kind === 'plain' ? fallbackLabels.limit : file.status === 'ready' ? `${rows.length} 行 · 只读${view?.structured ? ' · hunk 行号，仅供阅读' : ''}` : '只读；不支持保存或暂存写入'}</span>{file.status === 'ready' ? <button type="button" onClick={retry}>重新读取文件</button> : null}</div>
  </section>
}
