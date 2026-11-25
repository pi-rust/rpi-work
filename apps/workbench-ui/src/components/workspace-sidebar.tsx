import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Archive, ChevronDown, Folder, ListChecks, Network, PanelLeftClose, Pin, Plus, Search, SquarePen } from 'lucide-react'
import type { Project, Session, SessionActivityRead, SessionFilter } from '@/lib/types'
import type { ProjectGrouping } from '@/lib/project-grouping'
import { cn } from '@/lib/utils'

type Section = 'projects' | 'node' | 'pinned' | 'recent'
type Preferences = { folded: Partial<Record<Section, boolean>>; projectPins: string[] }
function readPreferences(scope: string): Preferences {
  try {
    const raw = JSON.parse(localStorage.getItem(`rpi.work.sidebar.v1:${scope}`) ?? '{}')
    const folded = Object.fromEntries(['projects', 'node', 'pinned', 'recent'].map(key => [key, raw?.folded?.[key] === true]))
    return { folded, projectPins: Array.isArray(raw?.projectPins) ? raw.projectPins.filter((id: unknown): id is string => typeof id === 'string').slice(0, 1000) : [] }
  } catch { return { folded: {}, projectPins: [] } }
}
type Props = {
  projectGrouping: ProjectGrouping
  projects: Project[]; sessions: Session[]; project: string; session: string
  filter: SessionFilter; onFilter?: (mode: SessionFilter) => void; activityRead: SessionActivityRead
  onProject: (id: string) => void; onSession: (id: string) => void
  onNewChat?: () => void; onAddProject?: () => void; creationHint?: string; onSearch?: () => void; onArchives?: () => void; onClose: () => void
  className?: string; connection?: ReactNode; nodeLabel: string; scope: string
  nodeState: 'fixture' | 'online' | 'offline' | 'disconnected'
}

/** Presentation-only preferences; session pin/archive facts remain node-owned. */
export function Sidebar({ projects, projectGrouping, sessions, project, session, filter, onFilter, activityRead, onProject, onSession, onNewChat, onAddProject, creationHint, onSearch, onArchives, onClose, className, connection, nodeLabel, nodeState, scope }: Props) {
  const [preferences, setPreferences] = useState(() => readPreferences(scope))
  const [filterOpen, setFilterOpen] = useState(false)
  const filterButton = useRef<HTMLButtonElement>(null)
  const filterSelect = useRef<HTMLSelectElement>(null)
  useEffect(() => { if (filterOpen) filterSelect.current?.focus() }, [filterOpen])
  const closeFilter = () => { setFilterOpen(false); filterButton.current?.focus() }

  function save(next: Preferences) {
    setPreferences(next)
    try { localStorage.setItem(`rpi.work.sidebar.v1:${scope}`, JSON.stringify(next)) } catch { /* In-memory preferences remain usable. */ }
  }
  const toggle = (section: Section) => save({ ...preferences, folded: { ...preferences.folded, [section]: !preferences.folded[section] } })
  const pinnedProjects = new Set(preferences.projectPins)
  const orderedProjects = [...projects].sort((a, b) => Number(pinnedProjects.has(b.id)) - Number(pinnedProjects.has(a.id)))
  const grouped = projectGrouping === 'node'
  const nodeDescription = `所属节点：${nodeLabel}${nodeState === 'fixture' ? '（示例）' : ''}`
  const projectRows = orderedProjects.map(item => (
    <div className="wb-project-row" key={item.id}>
      <button type="button" className="wb-project" onClick={() => onProject(item.id)} aria-current={project === item.id ? 'true' : undefined}
        aria-description={nodeDescription} title={`${item.name} · ${nodeDescription}`}>
        <Folder aria-hidden="true" /><span>{item.name}</span>
        {pinnedProjects.has(item.id) ? <Pin aria-hidden="true" className="wb-project-pin-marker" /> : null}
        {!grouped ? <Network aria-hidden="true" className="wb-project-node-icon" /> : null}
      </button>
      <button type="button" className="wb-icon wb-project-pin" aria-label={`置顶项目 ${item.name}`} aria-pressed={pinnedProjects.has(item.id)} title="仅调整本设备项目显示顺序，不改变会话"
        onClick={() => save({ ...preferences, projectPins: pinnedProjects.has(item.id) ? preferences.projectPins.filter(id => id !== item.id) : [...preferences.projectPins, item.id] })}><Pin aria-hidden="true" /></button>
    </div>
  ))
  const availableSessions = sessions.filter(item => !item.archived)
  const visibleSessions = availableSessions.filter(item => filter === 'all' || (activityRead === 'ready' &&
    (item.activity === 'awaitingApproval' || (filter === 'running' && item.activity === 'running'))))
    .sort((a, b) => b.updatedAt - a.updatedAt)
  const hasUnknownActivity = filter !== 'all' && activityRead === 'ready' && availableSessions.some(item => !item.activity || item.activity === 'unknown')
  const filterNotice = filter === 'all' ? null : activityRead === 'loading' ? '正在读取会话状态…'
    : activityRead === 'error' ? '会话状态读取失败，正在等待重试。'
    : hasUnknownActivity ? '部分会话状态未知。' : null
  const pinnedSessions = visibleSessions.filter(item => item.pinned)
  function heading(section: Section, label: string) {
    return <button type="button" className="wb-section-toggle" aria-label={section === 'projects' ? '项目列表' : `${label}列表`} aria-expanded={!preferences.folded[section]} aria-controls={`wb-${section}`} onClick={() => toggle(section)}><span>{label}</span><ChevronDown aria-hidden="true" /></button>
  }
  function threads(items: Session[]) {
    return items.map(item => <button key={item.id} type="button" className="wb-thread" aria-current={item.id === session ? 'true' : undefined} onClick={() => onSession(item.id)} title={item.title}><span>{item.title}</span></button>)
  }
  return (
    <aside className={cn('wb-sidebar', className)} aria-label="项目和会话">
      <div className="wb-brand"><strong data-testid="sidebar-brand">RPI Work</strong><div className="wb-sidebar-utilities">
        <button ref={filterButton} type="button" className="wb-icon" aria-label="筛选会话" title="筛选会话" disabled={!onFilter}
          aria-controls="wb-session-filter" aria-expanded={filterOpen} aria-pressed={filter !== 'all'} onClick={() => setFilterOpen(!filterOpen)}><ListChecks aria-hidden="true" /></button>
        <button type="button" className="wb-icon" aria-label="搜索项目与会话" title="搜索会话" onClick={onSearch}><Search aria-hidden="true" /></button>
        <button type="button" className="wb-icon" aria-label="收起会话侧栏" title="收起会话侧栏" onClick={onClose}><PanelLeftClose aria-hidden="true" /></button>
      </div><div id="wb-session-filter" className="wb-session-filter" hidden={!filterOpen}>
        <label htmlFor="wb-session-filter-mode">显示会话</label>
        <select id="wb-session-filter-mode" ref={filterSelect} value={filter} onChange={event => { onFilter?.(event.target.value as SessionFilter); closeFilter() }}
          onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); closeFilter() } }}>
          <option value="all">全部会话</option>
          <option value="running">执行中</option>
          <option value="approval">待审批</option>
        </select>
      </div></div>
      <button type="button" className="wb-new-chat" onClick={onNewChat} disabled={!onNewChat} title={!onNewChat ? creationHint ?? '请先连接节点并选择项目；不会清空当前草稿' : undefined}><SquarePen aria-hidden="true" />新对话</button>
      <nav className="wb-project-scroll" aria-label="项目与会话">
        <section aria-label="项目" className="wb-sidebar-section">
          <div className="wb-project-heading">{heading('projects', '项目')}<button type="button" className="wb-icon" aria-label="添加项目" onClick={onAddProject} disabled={!onAddProject} title={!onAddProject ? creationHint ?? '请先连接真实节点' : '添加目标节点上的已有项目'}><Plus aria-hidden="true" /></button></div>
          <div id="wb-projects" hidden={preferences.folded.projects} data-layout={projectGrouping}>
            {nodeState === 'disconnected' ? <p className="wb-empty">连接节点后显示项目</p> : <>
              {grouped ? <button type="button" className="wb-node-toggle" aria-label={`节点项目 ${nodeLabel}`} aria-expanded={!preferences.folded.node} aria-controls="wb-node" onClick={() => toggle('node')}><ChevronDown aria-hidden="true" /><Network aria-hidden="true" /><span>{nodeLabel}</span>{nodeState === 'fixture' ? <small>示例</small> : null}<small>{projects.length}</small></button> : null}
              <div id="wb-node" hidden={grouped && preferences.folded.node}>{projectRows}{!projects.length ? <p className="wb-empty">当前节点没有项目</p> : null}</div>
            </>}
          </div>
        </section>
        {pinnedSessions.length ? <section aria-label="置顶会话" className="wb-sidebar-section"><div className="wb-project-heading">{heading('pinned', '置顶')}</div><div id="wb-pinned" hidden={preferences.folded.pinned}>{threads(pinnedSessions)}</div></section> : null}
        <section aria-label="最近会话" className="wb-sidebar-section"><div className="wb-project-heading">{heading('recent', '最近')}</div><div id="wb-recent" hidden={preferences.folded.recent}>{threads(visibleSessions.filter(item => !item.pinned))}{filterNotice ? <p className="wb-empty" role="status">{filterNotice}</p> : null}{!visibleSessions.length && !filterNotice ? <p className="wb-empty">{filter === 'approval' ? '没有等待审批的会话' : filter === 'running' ? '没有正在执行的会话' : nodeState === 'disconnected' ? '连接节点后显示会话' : '当前节点没有会话'}</p> : null}</div></section>
      </nav>
      <div className="wb-sidebar-footer">
        {connection}
        <div className="wb-node-status" data-state={nodeState}><span aria-hidden="true" />{nodeState === 'fixture' ? '示例数据 · 非真实节点' : nodeState === 'online' ? nodeLabel : nodeState === 'disconnected' ? '未连接节点' : '连接未就绪'}</div>
        <button type="button" className="wb-archive" aria-haspopup="dialog" disabled={!onArchives} onClick={onArchives}><Archive aria-hidden="true" />已归档会话</button>
      </div>
    </aside>
  )
}
