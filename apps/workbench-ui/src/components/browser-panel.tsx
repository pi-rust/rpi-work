import { useRef, useState } from 'react'
import { ArrowLeft, ArrowRight, CircleHelp, ExternalLink, Globe, Plus, RotateCw, X } from 'lucide-react'

import './browser-panel.css'

type Tab = { id: number; history: string[]; index: number; address: string; revision: number; error: string | null; loading: boolean }
const blankTab = (id: number): Tab => ({ id, history: [], index: -1, address: '', revision: 0, error: null, loading: false })
function previewAddress(raw: string): string {
  const url = new URL(raw.trim())
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw Error('请输入不含账户或密码的完整 HTTP(S) 地址。')
  return url.href
}

/** A real, opaque-origin preview. No injected node credentials or auth bridge. */
export function BrowserPanel({ onClose }: { onClose: () => void }) {
  const nextId = useRef(2)
  const [tabs, setTabs] = useState<Tab[]>(() => [blankTab(1)])
  const [active, setActive] = useState(1)
  const [viewport, setViewport] = useState('responsive')
  const tab = tabs.find(item => item.id === active) ?? tabs[0]
  const url = tab.history[tab.index]
  function update(id: number, change: (value: Tab) => Tab) {
    setTabs(items => items.map(item => item.id === id ? change(item) : item))
  }
  function navigate(address: string) {
    try {
      const target = previewAddress(address)
      update(tab.id, value => ({ ...value, history: [...value.history.slice(0, value.index + 1), target], index: value.index + 1, address: target, loading: true, error: null, revision: value.revision + 1 }))
    } catch {
      update(tab.id, value => ({ ...value, error: '请输入不含账户或密码的完整 HTTP(S) 地址。' }))
    }
  }
  function move(index: number) {
    if (index < 0 || index >= tab.history.length) return
    update(tab.id, value => ({ ...value, index, address: value.history[index], loading: true, error: null, revision: value.revision + 1 }))
  }
  function addTab() {
    if (tabs.length >= 12) return
    const id = nextId.current++
    setTabs(items => [...items, blankTab(id)])
    setActive(id)
  }
  function closeTab(id: number) {
    if (tabs.length === 1) {
      const fresh = nextId.current++
      setTabs([blankTab(fresh)]); setActive(fresh)
      return
    }
    const index = tabs.findIndex(item => item.id === id)
    setTabs(items => items.filter(item => item.id !== id))
    if (active === id) setActive(tabs[index - 1]?.id ?? tabs[index + 1].id)
  }
  return (
    <section className="real-browser" aria-label="浏览器预览">
      <div className="real-browser-tabs" role="tablist" aria-label="浏览器标签页">
        {tabs.map(item => (
          <div className="real-browser-tab" key={item.id}>
            <button type="button" role="tab" id={`preview-tab-${item.id}`} aria-controls={`preview-page-${item.id}`} aria-selected={item.id === active} tabIndex={item.id === active ? 0 : -1} onClick={() => setActive(item.id)}
              onKeyDown={event => {
                if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
                event.preventDefault()
                const index = tabs.findIndex(value => value.id === item.id)
                const next = event.key === 'Home' ? tabs[0] : event.key === 'End' ? tabs[tabs.length - 1] : tabs[(index + (event.key === 'ArrowLeft' ? -1 : 1) + tabs.length) % tabs.length]
                setActive(next.id); document.getElementById(`preview-tab-${next.id}`)?.focus()
              }} title={item.history[item.index] ?? '新标签页'}>
              <Globe aria-hidden="true" /><span>{item.history[item.index] ? new URL(item.history[item.index]).hostname : '新标签页'}</span>
            </button>
            <button type="button" aria-label={`关闭预览标签页 ${item.id}`} onClick={() => closeTab(item.id)}><X aria-hidden="true" /></button>
          </div>
        ))}
        <button className="real-browser-icon" type="button" aria-label="新建浏览器标签页" title="新建标签页" disabled={tabs.length >= 12} onClick={addTab}><Plus aria-hidden="true" /></button>
        <button className="real-browser-icon real-browser-close-panel" type="button" aria-label="关闭浏览器面板" title="关闭面板" onClick={onClose}><X aria-hidden="true" /></button>
      </div>
      <form className="real-browser-toolbar" onSubmit={event => { event.preventDefault(); navigate(tab.address) }}>
        <button type="button" className="real-browser-icon" aria-label="预览后退" title="后退" disabled={tab.index <= 0} onClick={() => move(tab.index - 1)}><ArrowLeft aria-hidden="true" /></button>
        <button type="button" className="real-browser-icon" aria-label="预览前进" title="前进" disabled={tab.index + 1 >= tab.history.length} onClick={() => move(tab.index + 1)}><ArrowRight aria-hidden="true" /></button>
        <button type="button" className="real-browser-icon" aria-label="刷新预览" title="刷新" disabled={!url} onClick={() => update(tab.id, value => ({ ...value, revision: value.revision + 1, loading: true }))}><RotateCw aria-hidden="true" /></button>
        <input aria-label="预览地址" aria-invalid={Boolean(tab.error)} aria-describedby="preview-address-error" value={tab.address} onChange={event => update(tab.id, value => ({ ...value, address: event.target.value, error: null }))} placeholder="输入 HTTP(S) 预览地址…" autoComplete="off" spellCheck={false} />
        <button type="submit" className="real-browser-go" disabled={!tab.address.trim()}>打开</button>
        {url ? <a className="real-browser-icon" aria-label="在新窗口打开预览" title="在新窗口打开" href={url} target="_blank" rel="noopener noreferrer"><ExternalLink aria-hidden="true" /></a> : null}
      </form>
      <p id="preview-address-error" className="real-browser-error" role="alert" hidden={!tab.error}>{tab.error}</p>
      <div className="real-browser-stage" data-viewport={viewport}>
        {tabs.map(item => {
          const address = item.history[item.index]
          return (
            <div key={item.id} id={`preview-page-${item.id}`} role="tabpanel" aria-labelledby={`preview-tab-${item.id}`} hidden={item.id !== active} className="real-browser-page">
              {address ? <iframe key={`${item.id}:${item.revision}`} title={`页面预览 ${item.id}`} src={address} sandbox="allow-scripts" referrerPolicy="no-referrer" onLoad={() => update(item.id, value => value.revision === item.revision ? { ...value, loading: false } : value)} /> : (
                <div className="real-browser-empty"><Globe aria-hidden="true" /><strong>预览项目页面</strong><p>在地址栏输入项目页面的 HTTP(S) 地址。</p></div>
              )}
            </div>
          )
        })}
      </div>
      <div className="real-browser-controls">
        <span role="status">{url ? tab.loading ? '正在请求页面…' : '真实页面预览' : '尚未打开页面'}</span>
        <label>页面<select aria-label="预览页面历史" disabled={!url} value={tab.index} onChange={event => move(Number(event.target.value))}>{tab.history.length ? tab.history.map((address, index) => <option value={index} key={index}>{new URL(address).pathname || '/'} · {index + 1}</option>) : <option value={-1}>未打开</option>}</select></label>
        <label>视口<select aria-label="预览视口" value={viewport} onChange={event => setViewport(event.target.value)}><option value="responsive">自适应</option><option value="mobile">移动设备</option></select></label>
        <details className="real-browser-help">
          <summary aria-label="预览说明" title="预览说明"><CircleHelp aria-hidden="true" /></summary>
          <p>历史记录来自地址栏导航。若网站限制嵌入，请在新窗口打开。</p>
        </details>
      </div>
    </section>
  )
}
