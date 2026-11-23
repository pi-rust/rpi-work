import { Blocks, Clock, CodeXml, Folder, Globe, Settings2, Target, Terminal } from 'lucide-react'

type Props = { page: 'chat' | 'resources' | 'settings'; onChat: () => void; onProjects: () => void; onResources: () => void; onBrowser: () => void; onFiles: () => void; onSettings: () => void }
export function ActivityRail({ page, onChat, onProjects, onResources, onBrowser, onFiles, onSettings }: Props) {
  return <aside className="wb-activity-rail" aria-label="快捷导航">
    <nav aria-label="工作台快捷入口">
      <button type="button" className="wb-rail-button" aria-label="会话" title="会话" aria-current={page === 'chat' ? 'page' : undefined} onClick={onChat}><Terminal aria-hidden="true" /></button>
      <button type="button" className="wb-rail-button" aria-label="项目" title="项目" onClick={onProjects}><Folder aria-hidden="true" /></button>
      <button type="button" className="wb-rail-button" aria-label="会话目标" disabled title="会话目标尚未接入真实客户端"><Target aria-hidden="true" /></button>
      <button type="button" className="wb-rail-button" aria-label="计划任务" disabled title="计划任务尚未接入真实客户端"><Clock aria-hidden="true" /></button>
      <button type="button" className="wb-rail-button" aria-label="技能与插件" title="技能与插件" aria-current={page === 'resources' ? 'page' : undefined} onClick={onResources}><Blocks aria-hidden="true" /></button>
      <span className="wb-rail-separator" />
      <button type="button" className="wb-rail-button" aria-label="浏览器预览" title="浏览器预览" onClick={onBrowser}><Globe aria-hidden="true" /></button>
      <button type="button" className="wb-rail-button" aria-label="代码工作区" title="代码工作区" onClick={onFiles}><CodeXml aria-hidden="true" /></button>
    </nav>
    <button type="button" className="wb-rail-button wb-rail-settings" aria-label="工作台设置" title="设置" aria-current={page === 'settings' ? 'page' : undefined} onClick={onSettings}><Settings2 aria-hidden="true" /></button>
  </aside>
}
