/**
 * Shared workspace presentation backed by the existing Rust client.
 * Production starts empty; only a separate, explicit test build supplies data.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { sample } from '@/lib/initial-source'
import {
  NodeConnection,
  type BoundSession,
  type SessionControls,
  type SessionRecovery,
  type SessionDialog,
  type NodeChangedFile,
  type NodeProvider,
  type NodeFileEntry,
  type DirectoryPage,
  type NodeModel,
  type NodeQueue,
  type NodeRuntime,
  type NodeSession,
  type SessionManagementCapabilities,
  type SessionView,
} from '@/lib/node'
import {
  initViewModel,
  resourceRows,
  sealTicket,
  TranscriptPresenter,
  worktreeReviewPrompt,
  type ConversationView,
  type ResourceView,
  type RowRef,
} from '@/lib/viewmodel'
import type { ReviewDelivery } from '@/lib/review-delivery'
import type { Block, HistoryMessage, Project, Session, SessionActivityRead, SessionFilter } from '@/lib/types'

export type PendingDialog = SessionDialog
export type Source = { kind: 'disconnected' } | { kind: 'fixture' } | { kind: 'live'; origin: string }
export type Status = 'boot' | 'disconnected' | 'fixture' | 'connecting' | 'auth-required' | 'ready' | 'error'

export type FileRead = { path: string; mode: 'source' | 'diff'; staged: boolean } & (
  | { status: 'loading' }
  | { status: 'ready'; content: string }
  | { status: 'error'; error: string }
)

type SessionEdit = { archived: boolean } | { pinned: boolean } | { title: string } | { forkEntryId: string | null }
const initialSessionManagement = { capabilities: { preferences: false, rename: false, fork: false }, busy: false, message: null, locked: [], forkSession: null }

type Selection = { project: string; session: string }

export type Workspace = {
  presentationReady: boolean
  status: Status
  connectionId: number
  creation: { busy: boolean; uncertain: boolean; message: string | null }
  createProject: (name: string, path: string) => Promise<boolean>
  createSession: () => Promise<boolean>
  recheckCreation: () => Promise<boolean>
  sessionManagement: { capabilities: SessionManagementCapabilities; busy: boolean; message: string | null; locked: string[]; forkSession: string | null }
  forkAvailable: boolean
  forkFromEntry: boolean
  forkFeedback: string | null
  editSession: (id: string, edit: SessionEdit) => Promise<'selected' | 'saved' | 'failed'>
  source: Source
  error: string | null
  projects: Project[]
  sessions: Session[]
  sessionFilter: SessionFilter
  setSessionFilter: (filter: SessionFilter) => void
  sessionActivityRead: SessionActivityRead
  /** Canonical messages for the selected session (fixture: also projected). */
  history: HistoryMessage[]
  blocks: Block[]
  selected: Selection
  previousSelection: (() => void) | undefined
  nextSelection: (() => void) | undefined
  rows: RowRef[]
  transcript: ConversationView | null
  messageCount: number
  runtime: NodeRuntime | null
  models: NodeModel[]
  thinkingLevels: string[]
  busy: boolean
  controls: SessionControls | null
  recovery: SessionRecovery | null
  recheckSession: () => Promise<void>
  sendError: string | null
  queue: NodeQueue
  resources: ResourceView
  providers: NodeProvider[]
  authorization: { payload: string; expiresAt: number } | null
  authorizationError: string | null
  issueAuthorization: () => Promise<void>
  providerRevision: string | null
  providerError: string | null
  loadProviders: () => Promise<void>
  saveProviderKey: (id: string, key: string) => Promise<boolean>
  providerNotice: string | null
  nodeInfo: { version: string | null; service: string | null; protocol: string | null; nodeId: string | null } | null
  dialogs: PendingDialog[]
  answerDialog: (requestId: string, values: string[]) => Promise<void>
  loadNodeInfo: () => Promise<void>
  offers: unknown[]
  bundle: { manifest: { id: string; title: string; version: string }; sources: Record<string, string>; sha256: string } | null
  installing: boolean
  installError: string | null
  loadResources: () => Promise<void>
  reviewOffer: (offer: unknown) => Promise<void>
  cancelReview: () => void
  confirmInstall: (scope: 'global' | 'project') => Promise<void>
  changes: NodeChangedFile[]
  files: NodeFileEntry[]
  fileRead: FileRead | null
  changeReview: { pending: boolean; notice: string | null }
  reviewChanges: (delivery: ReviewDelivery) => Promise<void>
  loadChanges: () => Promise<void>
  loadFiles: (path?: string) => Promise<void>
  openChange: (path: string, staged?: boolean) => Promise<void>
  openFile: (path: string) => Promise<void>
  /** Read one directory; null means there is no longer a current scope. */
  verifyDiff: (path: string, staged: boolean) => Promise<string | null>
  listDir: (path: string, cursor?: string) => Promise<DirectoryPage | null>
  refreshQueue: () => Promise<void>
  queueMessage: (text: string) => Promise<boolean>
  cancelQueued: (entryId: string) => Promise<void>
  clearQueue: () => Promise<void>
  resumePending: () => Promise<void>
  send: (text: string) => Promise<boolean>
  abort: () => Promise<void>
  setModel: (model: NodeModel) => Promise<void>
  setThinking: (level: string) => Promise<void>
  connect: (origin: string) => Promise<void>
  submitpassword: (password: string) => Promise<void>
  disconnect: () => void
  selectProject: (id: string) => void
  selectSession: (id: string) => void
}

const asSession = (session: NodeSession): Session => ({
  id: session.sessionId,
  projectId: session.projectId ?? '',
  title: session.title ?? '(未命名会话)',
  activity: session.activity ?? undefined,
  updatedAt: session.updatedAt,
  pinned: session.pinned,
  archived: session.archived,
})

const historyFromView = (view?: ConversationView): HistoryMessage[] =>
  view?.parts.flatMap(part => part.kind === 'message' ? [{ id: part.key, role: part.role, content: part.text }] : []) ?? []

export function useWorkspace(): Workspace {
  const [status, setStatus] = useState<Status>('boot')
  const [presenter, setPresenter] = useState<TranscriptPresenter | null>(null)
  const presentationReady = presenter !== null
  const [source, setSource] = useState<Source>({ kind: sample ? 'fixture' : 'disconnected' })
  const [error, setError] = useState<string | null>(null)
  const [projects, setProjects] = useState<Project[]>(sample?.projects ?? [])
  const [sessions, setSessions] = useState<Session[]>(sample?.sessions ?? [])
  const [history, setHistory] = useState<HistoryMessage[]>(sample?.history ?? [])
  const [blocks, setBlocks] = useState<Block[]>(sample?.blocks ?? [])
  const [trail, setTrail] = useState(() => ({
    entries: [{ project: sample?.sessions[0]?.projectId ?? '', session: sample?.sessions[0]?.id ?? '' }],
    position: 0,
  }))
  const selected = trail.entries[trail.position]
  const connection = useRef<NodeConnection | null>(null)
  const epoch = useRef(0)
  const sessionTicket = useRef(0)
  const reviewTicket = useRef(0)
  const fileTicket = useRef(0)
  const listingTicket = useRef(0)
  const installRequest = useRef<object | null>(null)
  const [connectionId, setConnectionId] = useState(0)
  const [creation, setCreation] = useState<Workspace['creation']>({ busy: false, uncertain: false, message: null })
  type Creation = { conn: NodeConnection; epoch: number; ticket: number; kind: 'project' | 'session'; project?: string; id?: string; reading?: boolean }
  const creationRequest = useRef<Creation | null>(null)
  const [sessionManagement, setSessionManagement] = useState<Workspace['sessionManagement']>(initialSessionManagement)
  const sessionEditRequest = useRef<object | null>(null)
  const sessionEditLocks = useRef(new Map<string, Set<string>>())
  const sessionListVersion = useRef(0)
  const [filterSelection, setFilterSelection] = useState<{ connectionId: number; mode: SessionFilter }>({ connectionId: 0, mode: 'all' })
  const [activityRead, setActivityRead] = useState<SessionActivityRead>('loading')
  const sessionFilter = source.kind === 'live' && filterSelection.connectionId === connectionId ? filterSelection.mode : 'all'
  const setSessionFilter = (mode: SessionFilter) => {
    if (source.kind !== 'live') return
    if (sessionFilter === 'all') setActivityRead('loading')
    setFilterSelection({ connectionId, mode })
  }
  const pendingOrigin = useRef<string>('')
  // Read-only history may finish before the render that publishes the presenter.
  const presenterRef = useRef<TranscriptPresenter | null>(null)
  const historyRef = useRef<HistoryMessage[]>(sample?.history ?? [])
  const binding = useRef<BoundSession | null>(null)
  const bindingEpoch = useRef(0)
  const queueTicket = useRef(0)
  const observedControls = useRef<SessionControls | null>(null)
  const reviewRequest = useRef<object | null>(null)
  const [reviewPending, setReviewPending] = useState(false)
  const [reviewNotice, setReviewNotice] = useState<string | null>(null)
  const [controls, setControls] = useState<SessionControls | null>(null)
  const [recovery, setRecovery] = useState<SessionRecovery | null>(null)
  const [liveView, setLiveView] = useState<SessionView | null>(null)
  const [readOnlyTranscript, setReadOnlyTranscript] = useState<ConversationView | null>(null)
  const [runtime, setRuntime] = useState<NodeRuntime | null>(null)
  const [models, setModels] = useState<NodeModel[]>([])
  const [thinkingLevels, setThinkingLevels] = useState<string[]>([])
  const [queue, setQueue] = useState<NodeQueue>({ items: [], count: 0 })
  const [resources, setResources] = useState<ResourceView>({ rows: [], counts: [0, 0, 0] })
  const [providers, setProviders] = useState<NodeProvider[]>([])
  const [authorization, setAuthorization] = useState<{ payload: string; expiresAt: number } | null>(null)
  const [authorizationError, setAuthorizationError] = useState<string | null>(null)
  const [providerRevision, setProviderRevision] = useState<string | null>(null)
  const [providerError, setProviderError] = useState<string | null>(null)
  const [providerNotice, setProviderNotice] = useState<string | null>(null)
  const [nodeInfo, setNodeInfo] = useState<Workspace['nodeInfo']>(null)
  const [dialogs, setDialogs] = useState<PendingDialog[]>([])
  const [offers, setOffers] = useState<unknown[]>([])
  const [bundle, setBundle] = useState<Workspace['bundle']>(null)
  const [reviewedOffer, setReviewedOffer] = useState<unknown>(null)
  const [installing, setInstalling] = useState(false)
  const [installError, setInstallError] = useState<string | null>(null)
  const [changes, setChanges] = useState<NodeChangedFile[]>([])
  const [files, setFiles] = useState<NodeFileEntry[]>([])
  const [fileRead, setFileRead] = useState<FileRead | null>(null)
  const [sendError, setSendError] = useState<string | null>(null)
  const sessionIdRef = useRef<string>('')

  useEffect(() => {
    let active = true
    void initViewModel().then(() => {
      if (!active) return
      const instance = new TranscriptPresenter()
      presenterRef.current = instance
      setPresenter(instance)
      if (epoch.current === 0) setStatus(sample ? 'fixture' : 'disconnected')
    }).catch(cause => {
      if (!active) return
      setError(`显示组件加载失败：${String(cause)}`)
      setStatus('error')
    })
    return () => { active = false }
  }, [])

  const stopLive = useCallback(() => {
    binding.current?.close()
    binding.current = null
    bindingEpoch.current = 0
    observedControls.current = null
  }, [])

  // The visit and selection change together, before the async session read starts.
  const visit = useCallback((project: string, session = '', position?: number) => {
    setTrail(current => {
      if (!project && !session) return { entries: [{ project, session }], position: 0 }
      // A concurrent creation may have replaced the forward branch since render.
      const destination = position === undefined ? undefined : current.entries[position]
      if (position !== undefined && destination?.project === project && destination.session === session) {
        return { ...current, position }
      }
      const previous = current.entries[current.position]
      if (previous.project === project && previous.session === session) return current
      const entries = [...current.entries.slice(0, current.position + 1), { project, session }]
      return { entries, position: entries.length - 1 }
    })
  }, [])

  const clearSession = useCallback((project: string, session = '', position?: number) => {
    stopLive()
    sessionTicket.current++
    reviewTicket.current++
    fileTicket.current++
    listingTicket.current++
    installRequest.current = null
    sessionIdRef.current = ''
    historyRef.current = []
    visit(project, session, position)
    setHistory([])
    setBlocks([])
    setLiveView(null)
    setReadOnlyTranscript(null)
    setRuntime(null)
    setModels([])
    setThinkingLevels([])
    setQueue({ items: [], count: 0 })
    setDialogs([])
    setControls(null)
    setRecovery(null)
    setSendError(null)
    setOffers([])
    setBundle(null)
    setReviewedOffer(null)
    setInstalling(false)
    setInstallError(null)
    setChanges([])
    setFiles([])
    setFileRead(null)
    setReviewNotice(null)
    setResources({ rows: [], counts: [0, 0, 0] })
  }, [stopLive, visit])

  const readQueue = useCallback(async (session: BoundSession, current: () => boolean) => {
    const ticket = ++queueTicket.current
    const value = await session.queue()
    if (current() && ticket === queueTicket.current) setQueue(value)
  }, [])

  const loadSession = useCallback(async (conn: NodeConnection, sessionId: string, projectId: string, position?: number) => {
    if (conn !== connection.current) return
    clearSession(projectId, sessionId, position)
    const generation = epoch.current
    const ticket = sessionTicket.current
    const current = () => connection.current === conn && epoch.current === generation && sessionTicket.current === ticket
    setStatus('connecting')
    setError(null)
    try {
      await initViewModel()
      if (!current()) return
      sessionIdRef.current = sessionId
      let queueObservation: Pick<SessionView, 'scopeEpoch' | 'queueRevision'> | null = null
      let lastQueueRead = ''
      const refreshObservedQueue = () => {
        const session = binding.current
        if (!session || !queueObservation) return
        const { scopeEpoch, queueRevision } = queueObservation
        const key = `${scopeEpoch}:${queueRevision}`
        if (key === lastQueueRead) return
        lastQueueRead = key
        void readQueue(session, () => current() && bindingEpoch.current === scopeEpoch).catch(() => {})
      }
      const opened = await conn.bindSession(sessionId, view => {
        if (!current() || view.scopeEpoch < bindingEpoch.current) return
        bindingEpoch.current = view.scopeEpoch
        queueObservation = view.controls.queueRead ? view : null
        refreshObservedQueue()
        const messages = historyFromView(view.transcript)
        historyRef.current = messages
        setHistory(messages)
        setLiveView(view)
        setBlocks(view.blocks as Block[])
        observedControls.current = view.controls
        setControls(view.controls)
        setRecovery(view.recovery)
        setRuntime(view.runtime)
        setModels(view.models)
        setThinkingLevels(view.thinkingLevels)
        setDialogs(view.dialogs)
        setSendError(view.error)
        setStatus(view.controls.online ? 'ready' : 'connecting')
      })
      if (!current()) { opened.close(); return }
      binding.current = opened
      refreshObservedQueue()
    } catch (cause) {
      if (!current()) return
      // Failed/unknown access is still readable, never writable or a fabricated
      // run projection. No fallback session or sample metadata is substituted.
      setError(String(cause))
      try {
        const page = await conn.messages(sessionId)
        if (!current()) return
        const readOnly = presenterRef.current?.conversation(`readonly/${conn.origin}/${sessionId}`, page.messages, [])
        if (!readOnly) throw new Error('显示组件尚未就绪')
        const messages = historyFromView(readOnly)
        setReadOnlyTranscript(readOnly)
        historyRef.current = messages
        setHistory(messages)
        setStatus('ready')
      } catch (readError) {
        if (!current()) return
        setError(String(readError))
        setStatus('error')
      }
    }
  }, [clearSession, readQueue])

  const finishConnect = useCallback(async (conn: NodeConnection, generation: number) => {
    const [nodeProjects, nodeSessions] = await Promise.all([conn.projects(), conn.sessions()])
    if (connection.current !== conn || epoch.current !== generation) return
    const mappedSessions = nodeSessions.map(asSession)
    void conn.sessionManagementCapabilities().then(capabilities => {
      if (connection.current === conn && epoch.current === generation) {
        setSessionManagement({ ...initialSessionManagement, capabilities, locked: [...(sessionEditLocks.current.get(conn.origin) ?? [])] })
      }
    }).catch(() => {})
    setSource({ kind: 'live', origin: conn.origin })
    setProjects(nodeProjects.map(({ id, name }) => ({ id, name })))
    setSessions(mappedSessions)
    const first = mappedSessions.find(session => !session.archived)
    if (first) await loadSession(conn, first.id, first.projectId)
    else {
      clearSession(nodeProjects[0]?.id ?? '')
      setStatus('ready')
    }
  }, [clearSession, loadSession])

  const observeActivity = sessionFilter !== 'all'
  useEffect(() => {
    const conn = connection.current
    if (!observeActivity || !conn || creation.busy || sessionManagement.busy) return
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const current = () => !disposed && connection.current === conn && epoch.current === connectionId
    const refresh = async () => {
      const version = sessionListVersion.current
      try {
        const listed = await conn.sessions()
        if (current() && version === sessionListVersion.current) {
          setSessions(listed.map(asSession))
          setActivityRead('ready')
        }
      } catch {
        if (current() && version === sessionListVersion.current) setActivityRead('error')
      } finally {
        if (current()) timer = setTimeout(() => void refresh(), 2000)
      }
    }
    void refresh()
    return () => { disposed = true; clearTimeout(timer) }
  }, [observeActivity, connectionId, creation.busy, sessionManagement.busy])

  const openNode = useCallback(
    async (origin: string) => {
      const generation = ++epoch.current
      reviewRequest.current = null
      setReviewPending(false)
      setConnectionId(generation)
      clearSession('')
      setProjects([])
      setSessions([])
      setProviders([])
      setProviderRevision(null)
      setProviderNotice(null)
      setProviderError(null)
      setAuthorization(null)
      setAuthorizationError(null)
      setNodeInfo(null)
      setOffers([])
      setBundle(null)
      setReviewedOffer(null)
      setInstalling(false)
      setInstallError(null)
      setSource({ kind: 'disconnected' })
      connection.current = null
      creationRequest.current = null
      sessionEditRequest.current = null
      setSessionManagement(initialSessionManagement)
      setCreation({ busy: false, uncertain: false, message: null })
      setError(null)
      setStatus('connecting')
      pendingOrigin.current = origin
      try {
        const conn = await NodeConnection.open(origin)
        if (epoch.current !== generation) return
        connection.current = conn
        const auth = await conn.auth()
        if (connection.current !== conn || epoch.current !== generation) return
        if (auth.required && !auth.authenticated) {
          setStatus('auth-required')
          return
        }
        await finishConnect(conn, generation)
      } catch (cause) {
        if (epoch.current !== generation) return
        setError(String(cause))
        setStatus('error')
      }
    },
    [clearSession, finishConnect],
  )

  const connect = useCallback(
    async (origin: string) => {
      await openNode(origin)
    },
    [openNode],
  )

  const submitpassword = useCallback(
    async (password: string) => {
      const conn = connection.current
      const generation = epoch.current
      if (!conn) return
      setStatus('connecting')
      try {
        await conn.login(password)
        if (connection.current !== conn || epoch.current !== generation) return
        await openNode(conn.origin)
      } catch (cause) {
        if (connection.current !== conn || epoch.current !== generation) return
        setError(String(cause))
        setStatus('auth-required')
      }
    },
    [openNode],
  )

  // Promise scope only: execution phases and action admission are Rust-owned.
  const scopeForSession = useCallback((sessionId: string) => {
    const session = binding.current
    if (!session || epoch.current !== connectionId || sessionIdRef.current !== sessionId) return null
    const conn = connection.current
    const generation = connectionId
    const ticket = sessionTicket.current
    const observation = bindingEpoch.current
    return { session, current: () => binding.current === session && bindingEpoch.current === observation && connection.current === conn && epoch.current === generation && sessionTicket.current === ticket }
  }, [connectionId])
  const boundScope = useCallback(() => scopeForSession(selected.session), [scopeForSession, selected.session])

  const submitPrompt = useCallback(async (scope: NonNullable<ReturnType<typeof scopeForSession>>, text: string) => {
    setSendError(null)
    try {
      const reply = await scope.session.prompt(text)
      if (!scope.current()) return false
      if (!reply.accepted) setSendError(reply.error ?? '发送未被确认，草稿保留')
      return reply.accepted
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
      return false
    }
  }, [])

  const send = useCallback(async (text: string) => {
    const scope = boundScope()
    return scope && text.trim() ? submitPrompt(scope, text) : false
  }, [boundScope, submitPrompt])

  const refreshQueue = useCallback(async () => {
    const scope = boundScope()
    if (!scope) return
    try {
      await readQueue(scope.session, scope.current)
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
    }
  }, [boundScope, readQueue])

  const resumePending = useCallback(async () => {
    const scope = boundScope()
    if (!scope) return
    try { await scope.session.resumePending() }
    catch (cause) { if (scope.current()) setSendError(String(cause)) }
  }, [boundScope])

  const recheckSession = useCallback(async () => {
    const scope = boundScope()
    if (!scope) return
    try { await scope.session.recheck() }
    catch (cause) { if (scope.current()) setSendError(String(cause)) }
  }, [boundScope])

  const queueMessage = useCallback(async (text: string) => {
    const scope = boundScope()
    if (!scope || !text.trim()) return false
    try {
      const reply = await scope.session.queueMessage(text)
      if (!scope.current()) return false
      if (reply.accepted !== true) { setSendError('入队未被确认，草稿保留'); return false }
      await refreshQueue()
      return scope.current()
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
      return false
    }
  }, [boundScope, refreshQueue])

  const cancelQueued = useCallback(async (entryId: string) => {
    const scope = boundScope()
    if (!scope) return
    try {
      await scope.session.cancelQueued(entryId)
      if (scope.current()) await refreshQueue()
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
    }
  }, [boundScope, refreshQueue])

  const clearQueue = useCallback(async () => {
    const scope = boundScope()
    if (!scope) return
    try {
      await scope.session.clearQueue()
      if (scope.current()) await refreshQueue()
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
    }
  }, [boundScope, refreshQueue])

  const nodeScope = useCallback(() => {
    const conn = connection.current
    const generation = connectionId
    if (!conn || epoch.current !== generation) return null
    return { conn, current: () => connection.current === conn && epoch.current === generation }
  }, [connectionId])

  const projectScope = useCallback((project: string, session: string) => {
    const scope = nodeScope()
    const ticket = sessionTicket.current
    if (!scope || !project || sessionIdRef.current !== session) return null
    return { conn: scope.conn, current: () => scope.current() && sessionTicket.current === ticket && sessionIdRef.current === session }
  }, [nodeScope])

  const issueAuthorization = useCallback(async () => {
    const scope = nodeScope()
    if (!scope) return
    setAuthorizationError(null)
    try {
      const issued = await scope.conn.issueQrAuthorization()
      if (!scope.current()) return
      // The payload envelope is produced by the Rust codec, not by the UI.
      setAuthorization({ payload: sealTicket(issued.ticket), expiresAt: issued.expiresAt })
    } catch (cause) {
      if (scope.current()) setAuthorizationError(String(cause))
    }
  }, [nodeScope])

  const loadNodeInfo = useCallback(async () => {
    const scope = nodeScope()
    if (!scope) return
    try {
      const info = await scope.conn.nodeInfo()
      if (!scope.current()) return
      setNodeInfo({
        version: info.version,
        service: info.service,
        protocol: info.protocol,
        nodeId: info.nodeId,
      })
    } catch {
      // Health is informational; a failure only hides the version row.
    }
  }, [nodeScope])

  const loadProviders = useCallback(async () => {
    const scope = nodeScope()
    if (!scope) return
    setProviderError(null)
    try {
      const catalog = await scope.conn.providerCatalog()
      if (!scope.current()) return
      setProviders(catalog.providers)
      setProviderRevision(catalog.revision)
    } catch (cause) {
      if (scope.current()) setProviderError(String(cause))
    }
  }, [nodeScope])

  const answerDialog = useCallback(async (requestId: string, values: string[]) => {
    const scope = boundScope()
    if (!scope) return
    try {
      await scope.session.answer(requestId, values)
      // An ACK does not close a dialog; wait for the canonical observation.
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
    }
  }, [boundScope])

  const saveProviderKey = useCallback(
    async (id: string, key: string) => {
      const scope = nodeScope()
      if (!scope || !providerRevision) {
        setProviderError('还没有读取到 provider 目录，无法安全写入')
        return false
      }
      setProviderError(null)
      setProviderNotice(null)
      try {
        const receipt = await scope.conn.setProviderKey(id, key, providerRevision)
        if (!scope.current()) return false
        // The receipt carries the authoritative catalogue; use it as the truth.
        setProviders(receipt.providers)
        setProviderRevision(receipt.revision)
        setProviderNotice(
          receipt.requiresRestart
            ? '已保存；节点报告需要重启，当前 Agent 加载状态仍以会话观察为准。'
            : '已保存；节点未要求重启，不代表当前 Agent 已加载。',
        )
        return true
      } catch (cause) {
        if (scope.current()) setProviderError(String(cause))
        return false
      }
    },
    [providerRevision, nodeScope],
  )

  const loadResources = useCallback(async () => {
    const scope = projectScope(selected.project, selected.session)
    if (!scope) return
    try {
      const catalog = await scope.conn.nativeCatalog(selected.project)
      if (!scope.current()) return
      setResources(resourceRows(catalog, selected.project))
      const packages = await scope.conn.packageCatalog(selected.project)
      if (scope.current()) setOffers(packages.packages ?? [])
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
    }
  }, [selected.project, selected.session, projectScope])

  const reviewOffer = useCallback(
    async (offer: unknown) => {
      const scope = projectScope(selected.project, selected.session)
      if (!scope) return
      const ticket = ++reviewTicket.current
      const current = () => scope.current() && reviewTicket.current === ticket
      setBundle(null)
      setReviewedOffer(null)
      setInstallError(null)
      try {
        const review = await scope.conn.packageBundle(selected.project, offer)
        if (!current()) return
        setBundle(review)
        setReviewedOffer(offer)
      } catch (cause) {
        if (current()) setInstallError(String(cause))
      }
    },
    [selected.project, selected.session, projectScope],
  )

  const cancelReview = useCallback(() => {
    reviewTicket.current++
    setBundle(null)
    setReviewedOffer(null)
    setInstallError(null)
  }, [])

  const confirmInstall = useCallback(
    async (target: 'global' | 'project') => {
      const scope = projectScope(selected.project, selected.session)
      if (!scope || !reviewedOffer || !bundle || installRequest.current) return
      const request = {}
      const review = reviewTicket.current
      installRequest.current = request
      setInstalling(true)
      setInstallError(null)
      try {
        await scope.conn.installPackage(selected.project, reviewedOffer, target)
        if (!scope.current()) return
        if (reviewTicket.current === review) {
          setBundle(null)
          setReviewedOffer(null)
        }
        // Read the catalogue back: the receipt is not proof of installation.
        await loadResources()
      } catch (cause) {
        if (scope.current() && reviewTicket.current === review) setInstallError(String(cause))
      } finally {
        if (installRequest.current === request) {
          installRequest.current = null
          if (scope.current()) setInstalling(false)
        }
      }
    },
    [selected.project, selected.session, reviewedOffer, bundle, loadResources, projectScope],
  )

  const loadChanges = useCallback(async () => {
    const scope = projectScope(selected.project, selected.session)
    if (!scope) return
    try {
      const status = await scope.conn.gitStatus(selected.project)
      if (scope.current()) setChanges(status.files.slice(0, 200))
    } catch (cause) {
      if (scope.current()) setSendError(String(cause))
    }
  }, [selected.project, selected.session, projectScope])

  const loadFiles = useCallback(
    async (path = '') => {
      const scope = projectScope(selected.project, selected.session)
      if (!scope) return
      const ticket = ++listingTicket.current
      const current = () => scope.current() && listingTicket.current === ticket
      try {
        const listing = await scope.conn.files(selected.project, path)
        if (current()) setFiles(listing.entries)
      } catch (cause) {
        if (current()) setSendError(String(cause))
      }
    },
    [selected.project, selected.session, projectScope],
  )

  // Local read presentation only: the existing NodeConnection still owns every
  // request, and fileTicket + projectScope reject stale results across scopes.
  const readFile = useCallback(
    async (path: string, mode: 'source' | 'diff', staged = false) => {
      const scope = projectScope(selected.project, selected.session)
      if (!scope) return
      const ticket = ++fileTicket.current
      const current = () => scope.current() && fileTicket.current === ticket
      const target = { path, mode, staged }
      setFileRead({ ...target, status: 'loading' })
      try {
        const content = mode === 'diff'
          ? (await scope.conn.gitDiff(selected.project, path, staged)).patch
          : (await scope.conn.file(selected.project, path)).content
        if (current()) setFileRead({ ...target, status: 'ready', content })
      } catch (cause) {
        if (current()) setFileRead({ ...target, status: 'error', error: String(cause) })
      }
    },
    [selected.project, selected.session, projectScope],
  )
  const openChange = useCallback((path: string, staged = false) => readFile(path, 'diff', staged), [readFile])
  const openFile = useCallback((path: string) => readFile(path, 'source'), [readFile])

  const verifyDiff = useCallback(async (path: string, staged: boolean) => {
    const scope = projectScope(selected.project, selected.session)
    if (!scope || !selected.session) return null
    const ticket = fileTicket.current
    const reply = await scope.conn.gitDiff(selected.project, path, staged)
    if (!scope.current() || fileTicket.current !== ticket) return null
    return reply.patch
  }, [selected.project, selected.session, projectScope])

  const listDir = useCallback(
    async (path: string, cursor?: string) => {
      const scope = projectScope(selected.project, selected.session)
      if (!scope) return null
      try {
        const listing = await scope.conn.filesPage(selected.project, path, cursor)
        return scope.current() ? listing : null
      } catch (cause) {
        if (!scope.current()) return null
        throw cause
      }
    },
    [selected.project, selected.session, projectScope],
  )

  const abort = useCallback(async () => {
    const scope = boundScope()
    if (!scope) return
    try { await scope.session.abort() }
    catch (cause) { if (scope.current()) setSendError(String(cause)) }
  }, [boundScope])

  const setModel = useCallback(async (model: NodeModel) => {
    const scope = boundScope()
    if (!scope) return
    try { await scope.session.setModel(model.id, model.provider) }
    catch (cause) { if (scope.current()) setSendError(String(cause)) }
  }, [boundScope])

  const setThinking = useCallback(async (level: string) => {
    const scope = boundScope()
    if (!scope) return
    try { await scope.session.setThinking(level) }
    catch (cause) { if (scope.current()) setSendError(String(cause)) }
  }, [boundScope])

  const disconnect = useCallback(() => {
    stopLive()
    setConnectionId(++epoch.current)
    sessionTicket.current++
    creationRequest.current = null
    sessionEditRequest.current = null
    setSessionManagement(initialSessionManagement)
    reviewRequest.current = null
    setReviewPending(false)
    setReviewNotice(null)
    setCreation({ busy: false, uncertain: false, message: null })
    connection.current = null
    setLiveView(null)
    setRuntime(null)
    setQueue({ items: [], count: 0 })
    setResources({ rows: [], counts: [0, 0, 0] })
    setProviders([])
    setAuthorization(null)
    setAuthorizationError(null)
    setProviderRevision(null)
    setProviderError(null)
    setProviderNotice(null)
    setNodeInfo(null)
    setDialogs([])
    setOffers([])
    setBundle(null)
    setReviewedOffer(null)
    setInstallError(null)
    setChanges([])
    setFiles([])
    setFileRead(null)
    setModels([])
    setThinkingLevels([])
    setSendError(null)
    sessionIdRef.current = ''
    historyRef.current = []
    setSource({ kind: 'disconnected' })
    setProjects([])
    setSessions([])
    setHistory([])
    setBlocks([])
    visit('')
    setControls(null)
    setRecovery(null)
    setError(null)
    setStatus('disconnected')
  }, [stopLive, visit])

  // A committed create is not inserted optimistically. Reconcile by its exact
  // node-returned ID; an unknown outcome stays locked and is never re-posted.
  const readCreation = useCallback(async (request: Creation): Promise<boolean> => {
    if (request.reading) return false
    sessionListVersion.current++
    request.reading = true
    const current = () => connection.current === request.conn && epoch.current === request.epoch && creationRequest.current === request
    try {
      const [nodeProjects, nodeSessions] = await Promise.all([request.conn.projects(), request.conn.sessions()])
      if (!current()) return false
      setProjects(nodeProjects.map(({ id, name }) => ({ id, name })))
      const mapped = nodeSessions.map(asSession)
      setSessions(mapped)
      if (!request.id) {
        setCreation({ busy: false, uncertain: true, message: '已刷新列表，但创建结果仍未确认。请在节点核对，勿重复提交；重连不会取消原请求。' })
        return false
      }
      const verified = request.kind === 'project' ? nodeProjects.some(item => item.id === request.id) : mapped.some(item => item.id === request.id && item.projectId === request.project)
      if (!verified) throw new Error(`列表尚未包含节点返回的 ${request.id}`)
      const sameSelection = sessionTicket.current === request.ticket
      if (sameSelection) {
        if (request.kind === 'project') {
          clearSession(request.id)
          setStatus('ready')
        } else {
          const session = mapped.find(item => item.id === request.id)!
          await loadSession(request.conn, session.id, session.projectId)
        }
      }
      if (!current()) return false
      creationRequest.current = null
      setCreation({ busy: false, uncertain: false, message: null })
      return sameSelection && sessionTicket.current === request.ticket + 1
    } catch (cause) {
      if (current()) setCreation({ busy: false, uncertain: true, message: `创建已返回 ${request.id ?? '未知结果'}，但尚未核对：${String(cause)}。不要重复创建，请先核对列表。` })
      return false
    } finally {
      request.reading = false
    }
  }, [clearSession, loadSession])

  const create = useCallback(async (kind: 'project' | 'session', name = '', path = '', onOpened?: (id: string) => Promise<void>): Promise<boolean> => {
    const conn = connection.current
    if (!conn || source.kind !== 'live' || status !== 'ready' || creationRequest.current || sessionEditRequest.current) return false
    if (kind === 'session' && !projects.some(project => project.id === selected.project)) return false
    if (kind === 'project' && (!name.trim() || !path.trim())) {
      setCreation({ busy: false, uncertain: false, message: '请填写项目名称和目标节点上的目录路径。' })
      return false
    }
    const request: Creation = { conn, epoch: epoch.current, ticket: sessionTicket.current, kind, project: kind === 'session' ? selected.project : undefined }
    sessionListVersion.current++
    creationRequest.current = request
    const current = () => connection.current === conn && epoch.current === request.epoch && creationRequest.current === request
    setCreation({ busy: true, uncertain: false, message: '正在创建并核对节点列表；关闭窗口不会取消节点请求。' })
    try {
      const reply = kind === 'project' ? await conn.createProject(name.trim(), path) : await conn.createSession(selected.project)
      if (!current()) return false
      if (reply.outcome !== 'created') {
        if (reply.outcome === 'rejected') creationRequest.current = null
        const message = reply.outcome === 'rejected' && sessionTicket.current !== request.ticket ? null : `${reply.error}${reply.outcome === 'unknown' ? '。先前创建请求的结果未确认，勿重复提交，请先核对节点。' : ''}`
        setCreation({ busy: false, uncertain: reply.outcome === 'unknown', message })
        return false
      }
      request.id = typeof reply.value === 'string' ? reply.value : reply.value.id
      if (!request.id) throw new Error('节点未返回有效创建ID')
      const opened = await readCreation(request)
      if (opened && sessionTicket.current === request.ticket + 1) await onOpened?.(request.id)
      return opened
    } catch (cause) {
      if (current()) setCreation({ busy: false, uncertain: true, message: `创建结果未确认：${String(cause)}。勿重复提交，请先核对节点。` })
      return false
    }
  }, [source, status, projects, selected.project, readCreation])

  const createProject = useCallback((name: string, path: string) => create('project', name, path), [create])
  const createSession = useCallback(() => create('session'), [create])
  const recheckCreation = useCallback(async () => {
    const request = creationRequest.current
    if (!request || creation.busy || sessionEditRequest.current) return false
    setCreation({ busy: true, uncertain: true, message: '正在核对列表；不会重新发送创建请求。' })
    return readCreation(request)
  }, [creation.busy, readCreation])

  const reviewChanges = async (delivery: ReviewDelivery) => {
    const scope = projectScope(selected.project, selected.session)
    if (!scope || !selected.session || status !== 'ready' || reviewRequest.current || creationRequest.current || sessionEditRequest.current) return
    const request = {}
    reviewRequest.current = request
    setReviewPending(true)
    setReviewNotice(null)
    try {
      const git = await scope.conn.gitStatus(selected.project)
      if (!scope.current()) return
      setChanges(git.isRepository ? git.files.slice(0, 200) : [])
      if (!git.isRepository) {
        setReviewNotice('当前项目没有可审阅的 Git 工作区。')
        return
      }
      if (!git.files.length) {
        setReviewNotice('当前项目没有未提交的改动。')
        return
      }
      const text = worktreeReviewPrompt()
      if (delivery === 'detached') {
        const opened = await create('session', '', '', async id => {
          const destination = scopeForSession(id)
          if (!destination || !observedControls.current?.send) {
            setReviewNotice('已创建独立会话。当前尚不能提交审阅请求。')
            return
          }
          const accepted = await submitPrompt(destination, text)
          if (destination.current()) setReviewNotice(accepted ? '独立会话已受理审阅请求。' : '独立会话的审阅请求未获确认，请查看会话状态。')
        })
        if (!opened && scope.current()) setReviewNotice('独立审阅未启动，请查看会话创建状态。')
        return
      }
      // Reuse the selected session and its latest Rust admission observation.
      const admitted = observedControls.current
      const accepted = admitted?.send ? await send(text) : admitted?.queue ? await queueMessage(text) : false
      if (scope.current()) setReviewNotice(accepted
        ? admitted?.send ? '当前会话已受理审阅请求。' : '审阅请求已加入当前会话队列。'
        : '当前会话的审阅请求未获确认，请查看会话状态。')
    } catch (cause) {
      if (scope.current()) setReviewNotice(`无法读取项目改动：${String(cause)}`)
    } finally {
      if (reviewRequest.current === request) {
        reviewRequest.current = null
        setReviewPending(false)
      }
    }
  }

  const openSelection = (project: string, session = '', position?: number) => {
    if (source.kind === 'live' && connection.current && epoch.current === connectionId) {
      if (session) void loadSession(connection.current, session, project, position)
      else {
        clearSession(project, '', position)
        setError(null)
        setStatus('ready')
      }
    } else if (source.kind === 'fixture') {
      if (session) visit(project, session, position)
      else clearSession(project, '', position)
    }
  }

  const selectProject = (id: string) => {
    const session = sessions.find(item => item.projectId === id && !item.archived)
    openSelection(id, session?.id)
  }

  const selectSession = (id: string) => {
    const session = sessions.find(item => item.id === id)
    if (session) openSelection(session.projectId, session.id)
  }

  const forkAvailable = Boolean(sessionManagement.capabilities.fork && liveView?.forkAvailable &&
    !sessionManagement.busy && !creation.busy && !sessionManagement.locked.includes(selected.session))

  const editSession: Workspace['editSession'] = async (id, edit) => {
    const conn = connection.current
    const session = sessions.find(item => item.id === id)
    const archived = 'archived' in edit ? edit.archived : undefined
    const restoring = archived === false
    const forking = 'forkEntryId' in edit
    const forkScope = forking ? boundScope() : null
    const capabilities = sessionManagement.capabilities
    if (!conn || epoch.current !== connectionId || source.kind !== 'live' || status !== 'ready' ||
        !session || sessionEditRequest.current || creation.busy || creationRequest.current?.reading ||
        sessionEditLocks.current.get(conn.origin)?.has(id) ||
        (forking ? !forkAvailable || !forkScope || (edit.forkEntryId !== null && !liveView?.forkFromEntry)
          : 'title' in edit ? !capabilities.rename || !edit.title.trim() : !capabilities.preferences) ||
        (restoring ? !session.archived : selected.session !== id || session.archived) ||
        (archived === true && (!controls?.online || controls.busy))) return 'failed'
    const request = {}
    const ticket = sessionTicket.current
    sessionListVersion.current++
    sessionEditRequest.current = request
    setSessionManagement(state => ({
      ...state, busy: true, forkSession: forking ? id : null,
      message: forking ? '正在分叉并核对节点会话列表…' : null,
    }))
    const locked = sessionEditLocks.current.get(conn.origin) ?? new Set<string>()
    sessionEditLocks.current.set(conn.origin, locked)
    // Keep an in-flight or uncertain mutation protected even across a same-origin reconnect.
    locked.add(id)
    const current = () => connection.current === conn && epoch.current === connectionId && sessionEditRequest.current === request
    try {
      const reply = forking ? await forkScope!.session.fork(edit.forkEntryId, session.title)
        : 'title' in edit ? await conn.renameSession(id, edit.title)
        : await conn.setSessionPreferences(id, edit)
      if (!current()) return 'failed'
      if (reply.outcome === 'rejected') locked.delete(id)
      if (reply.outcome !== 'saved') {
        const detail = reply.outcome === 'unknown' ? '。结果尚未确认，本页暂不能重试此会话。' : ''
        setSessionManagement(state => ({ ...state, message: `${reply.error}${detail}`, locked: [...locked] }))
        return 'failed'
      }
      if ('sessionId' in reply.value) {
        const destination = reply.value.sessionId
        if (!destination || sessions.some(item => item.id === destination)) throw new Error('节点未返回新的分叉会话 ID')
        const listed = (await conn.sessions()).map(asSession)
        if (!current()) return 'failed'
        const branch = listed.find(item => item.id === destination && item.projectId === session.projectId && !item.archived)
        if (!branch) throw new Error(`列表尚未确认分叉会话 ${destination}`)
        setSessions(listed)
        locked.delete(id)
        setSessionManagement(state => ({ ...state, message: null, forkSession: null }))
        if (!forkScope?.current()) return 'saved'
        openSelection(branch.projectId, branch.id)
        return 'selected'
      }
      locked.delete(id)
      setSessions(items => items.map(item => item.id === id ? { ...item, ...reply.value } : item))
      if (archived === undefined || ticket !== sessionTicket.current) return 'saved'
      if (archived) {
        const next = sessions.find(item => item.id !== id && !item.archived)
        if (next) openSelection(next.projectId, next.id)
        else openSelection(session.projectId)
      } else openSelection(session.projectId, id)
      return 'selected'
    } catch (cause) {
      if (current()) setSessionManagement(state => ({ ...state, message: `${String(cause)}。结果尚未确认，本页暂不能重试此会话。`, locked: [...locked] }))
      return 'failed'
    } finally {
      if (current()) {
        sessionEditRequest.current = null
        setSessionManagement(state => ({ ...state, busy: false }))
      }
    }
  }

  const stepSelection = (direction: -1 | 1) => {
    for (let position = trail.position + direction; position >= 0 && position < trail.entries.length; position += direction) {
      const { project, session } = trail.entries[position]
      const available = session
        ? sessions.some(item => item.id === session && item.projectId === project && !item.archived)
        : projects.some(item => item.id === project)
      if (available) return () => openSelection(project, session, position)
    }
    return undefined
  }

  const rows = useMemo<RowRef[]>(() => {
    if (!presenter) return []
    if (liveView && source.kind === 'live') return liveView.rows
    if (status !== 'fixture' && status !== 'ready') return []
    const nodeScope = source.kind === 'live' ? `${connectionId}:${source.origin}` : 'fixture'
    return presenter.rows(`${nodeScope}/${selected.project}/${selected.session || 'none'}`, history, blocks)
  }, [presenter, liveView, source, connectionId, status, selected, history, blocks])

  const transcript = useMemo<ConversationView | null>(() => {
    if (!presenter) return null
    if (source.kind === 'live') return liveView?.transcript ?? readOnlyTranscript
    if (status !== 'fixture') return null
    return presenter.conversation(`fixture/${selected.project}/${selected.session || 'none'}`, history, blocks)
  }, [presenter, source, liveView, readOnlyTranscript, status, selected, history, blocks])

  return {
    presentationReady,
    transcript,
    status,
    connectionId,
    creation,
    createProject,
    createSession,
    recheckCreation,
    sessionManagement,
    forkAvailable,
    forkFromEntry: Boolean(sessionManagement.capabilities.fork && liveView?.forkFromEntry),
    forkFeedback: sessionManagement.forkSession === selected.session ? sessionManagement.message : null,
    editSession,
    source,
    error,
    projects,
    sessions,
    sessionFilter,
    setSessionFilter,
    sessionActivityRead: activityRead,
    history,
    blocks,
    selected,
    previousSelection: stepSelection(-1),
    nextSelection: stepSelection(1),
    rows,
    messageCount: history.length,
    runtime,
    models,
    thinkingLevels,
    busy: controls?.busy ?? false,
    controls,
    recovery,
    recheckSession,
    sendError,
    queue,
    resources,
    providers,
    authorization,
    authorizationError,
    issueAuthorization,
    providerRevision,
    providerError,
    loadProviders,
    saveProviderKey,
    providerNotice,
    nodeInfo,
    loadNodeInfo,
    dialogs,
    answerDialog,
    offers,
    bundle,
    installing,
    installError,
    loadResources,
    reviewOffer,
    cancelReview,
    confirmInstall,
    changes,
    files,
    fileRead,
    changeReview: { pending: reviewPending, notice: reviewNotice },
    reviewChanges,
    loadChanges,
    loadFiles,
    openChange,
    openFile,
    listDir,
    verifyDiff,
    refreshQueue,
    queueMessage,
    cancelQueued,
    clearQueue,
    resumePending,
    send,
    abort,
    setModel,
    setThinking,
    connect,
    submitpassword,
    disconnect,
    selectProject,
    selectSession,
  }
}
