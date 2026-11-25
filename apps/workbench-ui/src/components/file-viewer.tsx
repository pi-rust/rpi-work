import { useMemo } from 'react'
import { CopyTextButton } from '@/components/copy-text-button'
import { fileView } from '@/lib/viewmodel'
import type { FileRead } from '@/lib/workspace'
import './file-viewer.css'

/** A read-only projection, not an editor, review machine or git action owner. */
export function FileViewer({ file, retry }: { file: FileRead; retry: () => void }) {
  const content = file.status === 'ready' ? file.content : ''
  const diff = file.mode === 'diff'
  const rows = useMemo(() => fileView(content, file.path, diff), [content, file.path, diff])
  return <section className="wb-file-view" aria-label="文件读取" aria-busy={file.status === 'loading'}>
    <div className="wb-file-header">
      <div><h3>{file.path}</h3><span>{diff ? `原始 Diff · ${file.staged ? '已暂存' : '工作区'}` : '只读源码'}</span></div>
      {file.status === 'ready' ? <CopyTextButton text={content} label={diff ? '复制 Diff' : '复制源码'} /> : null}
    </div>
    {file.status === 'loading' ? <p className="wb-file-notice" role="status">正在读取 {file.path}…</p>
      : file.status === 'error' ? <div className="wb-file-notice" role="alert"><p>读取失败：{file.error}</p><button type="button" onClick={retry}>重试读取文件</button></div>
      : content === '' ? <p className="wb-file-notice">{diff ? '当前范围没有 Diff。' : '空文件'}</p>
      : <div className="wb-source-code" tabIndex={0} role="region" aria-label="只读文件内容">
        {rows.map((row, index) => <div key={index} className="wb-source-line" data-kind={row.kind}>
          {row.number !== null ? <span className="wb-source-number" aria-hidden="true">{row.number}</span> : null}
          <code>{row.tokens.map((token, tokenIndex) => <span key={tokenIndex} className={token.class || undefined}>{token.text}</span>)}</code>
        </div>)}
      </div>}
    <div className="wb-file-status"><span>{rows[0]?.kind === 'plain' ? '内容较大，按完整原文显示；行号与高亮已停用。' : file.status === 'ready' ? `${rows.length} 行 · 只读` : '只读；不支持保存或暂存写入'}</span>{file.status === 'ready' ? <button type="button" onClick={retry}>重新读取文件</button> : null}</div>
  </section>
}
