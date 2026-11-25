import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react'
import { Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { CopyTextButton } from '@/components/copy-text-button'
import { diffFeedbackDraft, diffView, fileView, type DiffView } from '@/lib/viewmodel'
import type { FileRead } from '@/lib/workspace'
import './file-viewer.css'

const fallbackLabels: Record<NonNullable<DiffView['fallback_reason']>, string> = {
  limit: '内容较大，按完整原文显示；行号与高亮已停用。',
  invalid: '此 Diff 未通过统一差异校验，按完整原文显示；不提供行号。',
  unsupported: '此 Diff 的格式暂不支持编号，按完整原文显示。',
  no_hunks: '此 Diff 没有可编号的 hunk，按完整原文显示。',
}
type ReadyRead = Extract<FileRead, { status: 'ready' }>
type FeedbackTarget = { file: ReadyRead; index: number; location: string }
type FeedbackActions = { verify: (path: string, staged: boolean) => Promise<string | null>; append: (text: string) => boolean }

function FeedbackDialog({ target, actions, onClose }: { target: FeedbackTarget; actions: FeedbackActions; onClose: (added: boolean) => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  const editor = useRef<HTMLTextAreaElement>(null)
  const active = useRef(false)
  const submitting = useRef(false)
  const [text, setText] = useState('')
  const [pending, setPending] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  useLayoutEffect(() => {
    const element = dialog.current
    active.current = true
    element?.showModal()
    editor.current?.focus()
    return () => { active.current = false; element?.close() }
  }, [])
  const close = (added = false) => {
    active.current = false
    dialog.current?.close()
    onClose(added)
  }
  const { file, index, location } = target
  return <dialog ref={dialog} className="wb-feedback-dialog" aria-labelledby="wb-feedback-title" aria-describedby="wb-feedback-location"
    onCancel={event => { event.preventDefault(); close() }}>
    <h3 id="wb-feedback-title">行内反馈</h3>
    <p id="wb-feedback-location">{location} · {file.staged ? '已暂存' : '工作区'} Diff</p>
    <form onSubmit={async event => {
      event.preventDefault()
      if (submitting.current || !text.trim()) return
      submitting.current = true
      setPending(true)
      setNotice(null)
      try {
        const fresh = await actions.verify(file.path, file.staged)
        if (!active.current) return
        if (fresh === null) {
          setNotice('文件或会话已切换，请重新选择反馈行。')
          return
        }
        const addition = diffFeedbackDraft(file.content, fresh, file.path, file.staged, index, text)
        if (!addition) setNotice('Diff 已变化。请关闭反馈，重新读取文件后选择对应行。')
        else if (!actions.append(addition)) setNotice('草稿超过长度限制。请缩短草稿或反馈后再试。')
        else close(true)
      } catch (cause) {
        if (active.current) setNotice(`无法核对 Diff：${cause}`)
      } finally {
        submitting.current = false
        if (active.current) setPending(false)
      }
    }}>
      <label htmlFor="wb-feedback-text">反馈内容</label>
      <textarea id="wb-feedback-text" ref={editor} value={text} onChange={event => setText(event.target.value)} required maxLength={2000} readOnly={pending} />
      {notice ? <p role="alert">{notice}</p> : null}
      <div className="wb-feedback-actions"><Button type="button" variant="outline" onClick={() => close()}>取消</Button><Button type="submit" disabled={pending || !text.trim()}>{pending ? '正在核对…' : '添加到草稿'}</Button></div>
    </form>
  </dialog>
}

export function FileViewer({ file, retry, feedback }: { file: FileRead; retry: () => void; feedback?: FeedbackActions }) {
  const [addedTo, setAddedTo] = useState<FileRead | null>(null)
  const [target, setTarget] = useState<FeedbackTarget | null>(null)
  const content = file.status === 'ready' ? file.content : ''
  const diff = file.mode === 'diff'
  const view = useMemo(() => diff ? diffView(content, file.path) : null, [content, file.path, diff])
  const eligible = view?.feedback_eligible
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
          const location = numbered ? `${file.path} · ${next === null ? `旧行 ${old}` : `新行 ${next}`}` : null
          return <div key={index} className={`wb-source-line${numbered ? ' wb-diff-row' : ''}`} data-kind={row.kind}>
            {row.number !== null ? <span className="wb-source-number" aria-hidden="true">{row.number}</span> : null}
            {numbered ? <>
              <span className="wb-diff-number wb-diff-old" aria-hidden="true">{old}</span>
              <span className="wb-diff-number wb-diff-new" aria-hidden="true">{next}</span>
              <span className="sr-only">{old !== null ? `旧行 ${old}。` : ''}{next !== null ? `新行 ${next}。` : ''}</span>
            </> : null}
            <code>{row.tokens.map((token, tokenIndex) => <span key={tokenIndex} className={token.class || undefined}>{token.text}</span>)}</code>
            {feedback && eligible && location && file.status === 'ready' ? <button type="button" className="wb-feedback-line" aria-label={`在 ${location} 添加反馈`}
              title={`在 ${location} 添加反馈`} onClick={() => setTarget({ file, index, location })}><Plus aria-hidden="true" /></button> : null}
          </div>
        })}
      </div>}
    <div className="wb-file-status"><span>{view?.fallback_reason ? fallbackLabels[view.fallback_reason] : rows[0]?.kind === 'plain' ? fallbackLabels.limit : file.status === 'ready' ? `${rows.length} 行 · 只读${view?.structured ? eligible && feedback ? ' · 可添加行内反馈' : ' · hunk 行号，仅供阅读' : ''}` : '只读；不支持保存或暂存写入'}</span>{file.status === 'ready' ? <button type="button" onClick={retry}>重新读取文件</button> : null}</div>
    {addedTo === file ? <p className="wb-feedback-confirmed" role="status">已添加到当前会话草稿</p> : null}
    {feedback && target?.file === file ? <FeedbackDialog target={target} actions={feedback} onClose={added => {
      if (added) setAddedTo(file)
      setTarget(null)
    }} /> : null}
  </section>
}
