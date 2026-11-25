import { useEffect, useRef, useState } from 'react'
import { X } from 'lucide-react'
import type { Project, Session } from '@/lib/types'
import type { Workspace } from '@/lib/workspace'
import './project-dialog.css'

type Props = {
  mode: 'list' | 'current'
  sessions: Session[]
  projects: Project[]
  current: Session | undefined
  management: Workspace['sessionManagement']
  canArchive: boolean
  disabled: boolean
  onChange: Workspace['editSession']
  onSelected: () => void
  onClose: () => void
}

export function SessionManagementDialog({ mode, sessions, projects, current, management, canArchive, disabled, onChange, onSelected, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null)
  const [title, setTitle] = useState(current?.title ?? '')
  useEffect(() => {
    const element = dialog.current
    element?.showModal()
    return () => element?.close()
  }, [])
  const dismiss = () => { dialog.current?.close(); onClose() }
  const change = async (...args: Parameters<Workspace['editSession']>) => {
    const result = await onChange(...args)
    if (result === 'failed' || !dialog.current?.open) return
    dismiss()
    if (result === 'selected') onSelected()
  }
  const entries = sessions.filter(session => session.archived)
  const projectNames = new Map(projects.map(project => [project.id, project.name]))
  const currentDisabled = disabled || Boolean(current && management.locked.includes(current.id))
  return <dialog ref={dialog} className="wb-project-dialog" aria-labelledby="wb-archive-title" onCancel={event => { event.preventDefault(); dismiss() }}>
    <div className="wb-dialog-heading">
      <h2 id="wb-archive-title">{mode === 'list' ? '已归档会话' : '会话操作'}</h2>
      <button type="button" autoFocus className="wb-icon" aria-label="关闭会话窗口" onClick={dismiss}><X aria-hidden="true" /></button>
    </div>
    {mode === 'list' ? entries.length ? <ul className="text-[13px]">
      {entries.map(session => <li key={session.id} className="flex items-center justify-between gap-2 py-1.5">
        <span className="min-w-0 wrap-anywhere">{projectNames.get(session.projectId) ?? session.projectId}{session.projectId ? ' / ' : ''}{session.title}</span>
        <button type="button" disabled={disabled || !management.capabilities.preferences || management.locked.includes(session.id)} aria-label={`恢复 ${session.title}`} onClick={() => void change(session.id, { archived: false })}>恢复</button>
      </li>)}
    </ul> : <p className="text-muted-foreground text-[13px]">没有已归档的会话。</p> : current ? <>
      <form className="wb-dialog-form" onSubmit={event => {
        event.preventDefault()
        if (!currentDisabled && management.capabilities.rename && title.trim() && title.trim() !== current.title) {
          void change(current.id, { title })
        }
      }}>
        <label htmlFor="wb-session-name">会话名称</label>
        <div className="wb-project-path-row">
          <input id="wb-session-name" value={title} onChange={event => setTitle(event.target.value)} required maxLength={100} disabled={currentDisabled || !management.capabilities.rename} />
          <button type="submit" className="wb-dialog-primary" disabled={currentDisabled || !management.capabilities.rename || !title.trim() || title.trim() === current.title}>保存</button>
        </div>
      </form>
      <div className="flex flex-wrap gap-2">
        <button type="button" disabled={currentDisabled || !management.capabilities.preferences} onClick={() => void change(current.id, { pinned: !current.pinned })}>{current.pinned ? '取消置顶' : '置顶会话'}</button>
        <button type="button" disabled={currentDisabled || !canArchive} onClick={() => void change(current.id, { archived: true })}>归档会话</button>
      </div>
    </> : null}
    {management.busy ? <p role="status" className="mt-2 text-[13px]">正在保存会话设置…</p> : null}
    {management.message ? <p role="alert" className="mt-2 wrap-anywhere text-[13px]">{management.message}</p> : null}
    {mode === 'current' && !management.capabilities.rename ? <p className="text-muted-foreground mt-2 text-[13px]">当前节点尚未确认支持修改会话名称。</p> : null}
    {!management.capabilities.preferences ? <p className="text-muted-foreground mt-2 text-[13px]">当前节点尚未确认支持修改会话归档状态。</p> : null}
    {!management.message && (mode === 'list' ? entries.some(session => management.locked.includes(session.id)) : current && management.locked.includes(current.id)) ?
      <p className="text-muted-foreground mt-2 text-[13px]">{mode === 'list' ? '先前的归档结果尚未确认，本页暂不能重试该会话。' : '先前的会话操作结果尚未确认，本页暂不能重试该会话。'}</p> : null}
  </dialog>
}
