import { useEffect, useRef, useState } from 'react'
import { Folder, X } from 'lucide-react'
import './project-dialog.css'

type Props = {
  nodeLabel: string
  busy: boolean
  uncertain: boolean
  message: string | null
  onCreate: (name: string, path: string) => Promise<boolean>
  onRecheck: () => Promise<boolean>
  onClose: () => void
}

/** Same form as the confirmed prototype; paths belong to the target node. */
export function ProjectDialog({ nodeLabel, busy, uncertain, message, onCreate, onRecheck, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null)
  const active = useRef(false)
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  useEffect(() => {
    active.current = true
    const element = dialog.current
    element?.showModal()
    return () => { active.current = false; element?.close() }
  }, [])
  const dismiss = () => { dialog.current?.close(); onClose() }
  return <dialog ref={dialog} className="wb-project-dialog" aria-labelledby="wb-project-dialog-title" aria-describedby="wb-project-description" onCancel={event => { event.preventDefault(); dismiss() }}>
    <div className="wb-dialog-heading"><h2 id="wb-project-dialog-title">添加项目</h2><button type="button" className="wb-icon" aria-label="关闭添加项目" onClick={dismiss}><X aria-hidden="true" /></button></div>
    <p id="wb-project-description">将已有项目加入工作台，添加后即可开始对话。</p>
    <form className="wb-dialog-form" onSubmit={async event => {
      event.preventDefault()
      if (busy || uncertain) return
      const created = await onCreate(name, path)
      if (created && active.current) dismiss()
    }}>
      <label htmlFor="wb-project-name">名称</label><input id="wb-project-name" autoFocus required maxLength={50} placeholder="my-project" value={name} onChange={event => setName(event.target.value)} disabled={busy || uncertain} />
      <label htmlFor="wb-project-node">所属节点</label><select id="wb-project-node" aria-describedby="wb-project-help" disabled><option>{nodeLabel}</option></select>
      <label htmlFor="wb-project-path">项目路径</label><div className="wb-project-path-row"><input id="wb-project-path" required maxLength={300} placeholder="目标节点上的已有目录路径" value={path} onChange={event => setPath(event.target.value)} disabled={busy || uncertain} aria-describedby="wb-project-help" /><button type="button" disabled title="当前 Web 客户端没有目标节点的系统目录选择能力"><Folder aria-hidden="true" />选择文件夹</button></div>
      <p id="wb-project-help">填写目标节点上已存在的目录路径。浏览器不提供该节点的系统文件选择器；不会扫描本机、上传目录、创建目录或初始化 Git。</p>
      {message ? <div className="wb-creation-message" role={busy ? 'status' : 'alert'}><p>{message}</p>{uncertain ? <button type="button" disabled={busy} onClick={async () => { if (await onRecheck() && active.current) dismiss() }}>核对创建结果</button> : null}</div> : null}
      <div className="wb-dialog-actions"><button type="button" onClick={dismiss}>取消</button><button type="submit" disabled={busy || uncertain} className="wb-dialog-primary">{busy ? '正在添加…' : '添加项目'}</button></div>
    </form>
  </dialog>
}
