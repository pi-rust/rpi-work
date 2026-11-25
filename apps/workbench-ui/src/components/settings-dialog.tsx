import { useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { ChevronDown, Globe, Info, KeyRound, Palette, Search, Settings2, X } from 'lucide-react'
import './settings-dialog.css'

export type SettingsCategory = 'general' | 'providers' | 'connections' | 'appearance' | 'about'
const destinations = [
  { id: 'general', label: '常规', group: '应用', terms: '工作区 项目列表显示 按节点分组 平铺 审查结果呈现方式 审阅 当前会话 独立会话', icon: Settings2 },
  { id: 'appearance', label: '外观', group: '应用', terms: '主题 明暗 密度 阅读 经典 消息布局', icon: Palette },
  { id: 'providers', label: '提供商', group: '节点', terms: '配置 API 密钥 模型 provider revision', icon: KeyRound },
  { id: 'connections', label: '工作协同', group: '节点', terms: '节点 地址 项目 会话 手机 二维码 一次性授权', icon: Globe },
  { id: 'about', label: '关于', group: '应用信息', terms: '版本 应用更新 协议 许可证', icon: Info },
] as const

/** Native modality supplies the focus boundary and inert background. Only
 * existing device preferences and real node capabilities enter this shell. */
export function SettingsDialog({ onClose, general, appearance, children }: {
  onClose: () => void
  general: ReactNode
  appearance: ReactNode
  children: (category: SettingsCategory) => ReactNode
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const returnFocus = useRef<HTMLElement | null>(null)
  const content = useRef<HTMLDivElement>(null)
  const [category, setCategory] = useState<SettingsCategory>('providers')
  const [query, setQuery] = useState('')
  const [browse, setBrowse] = useState(false)
  useLayoutEffect(() => {
    const element = dialog.current
    returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
    element?.showModal()
    const search = element?.querySelector<HTMLInputElement>('input[type="search"]')
    if (search?.getClientRects().length) search.focus({ preventScroll: true })
    const repairFocus = () => {
      const focused = document.activeElement
      if (focused instanceof HTMLElement && element?.contains(focused) && !focused.getClientRects().length) {
        element.querySelector<HTMLButtonElement>('.wb-settings-browse')?.focus({ preventScroll: true })
      }
    }
    window.addEventListener('resize', repairFocus)
    return () => { window.removeEventListener('resize', repairFocus); element?.close() }
  }, [])
  const dismiss = () => {
    dialog.current?.close()
    const target = returnFocus.current
    if (target?.getClientRects().length) target.focus({ preventScroll: true })
    else document.querySelector<HTMLButtonElement>('[aria-label="打开显示选项"]')?.focus({ preventScroll: true })
    onClose()
  }
  const visible = destinations.filter(item => `${item.label} ${item.terms}`.toLowerCase().includes(query.trim().toLowerCase()))
  return <dialog ref={dialog} className="wb-settings-dialog" aria-labelledby="wb-settings-title" aria-describedby="wb-settings-description"
    onKeyDown={event => {
      if (event.key !== 'Tab') return
      const targets = [...event.currentTarget.querySelectorAll<HTMLElement>('button, input, select, textarea, a[href], [tabindex]')]
        .filter(node => node.tabIndex >= 0 && !node.matches(':disabled') && node.getClientRects().length > 0)
      const first = targets[0], last = targets.at(-1)
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
    }}
    onCancel={event => { event.preventDefault(); dismiss() }}>
    <div className="wb-settings-layout">
      <nav className="wb-settings-nav" aria-label="设置分类">
        <div className="wb-settings-heading">
          <h2 id="wb-settings-title">设置</h2>
          <button type="button" className="wb-settings-browse" aria-controls="wb-settings-destinations" aria-expanded={browse} onClick={() => setBrowse(!browse)}>浏览分类<ChevronDown aria-hidden="true" /></button>
          <button type="button" className="wb-settings-close" aria-label="关闭设置" onClick={dismiss}><X aria-hidden="true" /></button>
        </div>
        <div id="wb-settings-destinations" className="wb-settings-destinations" data-browse={browse}>
          <label className="wb-settings-search"><Search aria-hidden="true" /><input type="search" aria-label="搜索设置" placeholder="搜索" value={query} onChange={event => setQuery(event.target.value)} /></label>
          <div className="wb-settings-nav-scroll">
            {['应用', '节点', '应用信息'].map(group => {
              const items = visible.filter(item => item.group === group)
              return items.length ? <div className="wb-settings-nav-group" key={group}>
                <h3>{group}</h3>
                {items.map(item => <button key={item.id} type="button" aria-current={category === item.id ? 'page' : undefined}
                  onClick={() => {
                    setCategory(item.id)
                    setBrowse(false)
                    if (content.current) {
                      content.current.scrollTop = 0
                      if (matchMedia('(max-width: 700px)').matches) content.current.focus({ preventScroll: true })
                    }
                  }}>
                  <item.icon aria-hidden="true" /><span>{item.label}</span>
                </button>)}
              </div> : null
            })}
            {!visible.length ? <p className="wb-settings-empty" role="status">没有匹配的设置</p> : null}
          </div>
        </div>
      </nav>
      <div ref={content} tabIndex={-1} className="wb-settings-content">
        <p id="wb-settings-description" className="sr-only">本设备的显示偏好与当前节点的设置。</p>
        {category === 'general' ? general : null}
        {category === 'appearance' ? appearance : null}
        {children(category)}
      </div>
    </div>
  </dialog>
}
