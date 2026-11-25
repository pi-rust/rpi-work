import { useEffect, useRef } from 'react'
import { X } from 'lucide-react'
import type { Project, Session } from '@/lib/types'
import type { Workspace } from '@/lib/workspace'
import './project-dialog.css'

type Props = {
  mode: 'list' | 'current'
  sessions: Session[]
  projects: Project[]
  current: Session | undefined
  archive: Workspace['archive']
  canArchive: boolean
  disabled: boolean
  onChange: Workspace['setArchived']
  onSelected: () => void
  onClose: () => void
}

export function SessionArchiveDialog({ mode, sessions, projects, current, archive, canArchive, disabled, onChange, onSelected, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const element = dialog.current
    element?.showModal()
    return () => element?.close()
  }, [])
  const dismiss = () => { dialog.current?.close(); onClose() }
  const change = async (id: string, archived: boolean) => {
    const result = await onChange(id, archived)
    if (result === 'failed' || !dialog.current?.open) return
    dismiss()
    if (result === 'selected') onSelected()
  }
  const entries = sessions.filter(session => session.archived)
  const projectNames = new Map(projects.map(project => [project.id, project.name]))
  return <dialog ref={dialog} className="wb-project-dialog" aria-labelledby="wb-archive-title" onCancel={event => { event.preventDefault(); dismiss() }}>
    <div className="wb-dialog-heading">
      <h2 id="wb-archive-title">{mode === 'list' ? '已归档会话' : '会话操作'}</h2>
      <button type="button" autoFocus className="wb-icon" aria-label="关闭会话窗口" onClick={dismiss}><X aria-hidden="true" /></button>
    </div>
    {mode === 'list' ? entries.length ? <ul className="text-[13px]">
      {entries.map(session => <li key={session.id} className="flex items-center justify-between gap-2 py-1.5">
        <span className="min-w-0 wrap-anywhere">{projectNames.get(session.projectId) ?? session.projectId}{session.projectId ? ' / ' : ''}{session.title}</span>
        <button type="button" disabled={disabled || !archive.supported || archive.locked.includes(session.id)} aria-label={`恢复 ${session.title}`} onClick={() => void change(session.id, false)}>恢复</button>
      </li>)}
    </ul> : <p className="text-muted-foreground text-[13px]">没有已归档的会话。</p> : current ? <div className="flex items-center justify-between gap-2 text-[13px]">
      <span className="min-w-0 wrap-anywhere">{current.title}</span>
      <button type="button" disabled={disabled || !canArchive || archive.locked.includes(current.id)} onClick={() => void change(current.id, true)}>归档会话</button>
    </div> : null}
    {archive.busy ? <p role="status" className="mt-2 text-[13px]">正在保存会话归档状态…</p> : null}
    {archive.message ? <p role="alert" className="mt-2 wrap-anywhere text-[13px]">{archive.message}</p> : null}
    {!archive.supported ? <p className="text-muted-foreground mt-2 text-[13px]">当前节点尚未确认支持修改会话归档状态。</p> : null}
    {!archive.message && (mode === 'list' ? entries.some(session => archive.locked.includes(session.id)) : current && archive.locked.includes(current.id)) ?
      <p className="text-muted-foreground mt-2 text-[13px]">先前的归档结果尚未确认，本页暂不能重试该会话。</p> : null}
  </dialog>
}
