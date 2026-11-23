/**
 * Shared workspace presentation backed by the existing Rust client.
 * Production starts empty; only a separate, explicit test build supplies data.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { sample } from '@/lib/initial-source'
import {
  NodeConnection,
  type NodeChangedFile,
  type NodeProvider,
  type NodeFileEntry,
  type NodeModel,
  type NodeQueue,
  type NodeRuntime,
  type NodeSession,
} from '@/lib/node'
import {
  initViewModel,
  resourceRows,
  sealTicket,
  TranscriptPresenter,
  type LiveView,
  type ResourceView,
  type RowRef,
} from '@/lib/viewmodel'
import type { Block, HistoryMessage, Project, Session } from '@/lib/types'

export type PendingDialog = {
  requestId: string
  purpose: string | null
  question: string | null
  options: string[]
}
export type Source = { kind: 'disconnected' } | { kind: 'fixture' } | { kind: 'live'; origin: string }
export type Status = 'boot' | 'disconnected' | 'fixture' | 'connecting' | 'auth-required' | 'ready' | 'error'

export type Workspace = {
  presentationReady: boolean
  status: Status
  connectionId: number
  creation: { busy: boolean; uncertain: boolean; message: string | null }
  createProject: (name: string, path: string) => Promise<boolean>
  createSession: () => Promise<boolean>
  recheckCreation: () => Promise<boolean>
  source: Source
  error: string | null
  projects: Project[]
  sessions: Session[]
  /** Canonical messages for the selected session (fixture: also projected). */
  history: HistoryMessage[]
  blocks: Block[]
  selected: { project: string; session: string }
  rows: RowRef[]
  messageCount: number
  runtime: NodeRuntime | null
  models: NodeModel[]
  thinkingLevels: string[]
  busy: boolean
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
  filePath: string | null
  fileBody: string | null
  fileDiff: string | null
  loadChanges: () => Promise<void>
  loadFiles: (path?: string) => Promise<void>
  openChange: (path: string, staged?: boolean) => Promise<void>
  openFile: (path: string) => Promise<void>
  /** Read one directory without touching the panel's own listing. */
  listDir: (path: string) => Promise<NodeFileEntry[]>
  refreshQueue: () => Promise<void>
  queueMessage: (text: string) => Promise<boolean>
  cancelQueued: (entryId: string) => Promise<void>
  clearQueue: () => Promise<void>
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
  updatedAt: session.updatedAt,
  pinned: session.pinned,
  archived: session.archived,
})

const asHistory = (messages: unknown[]): HistoryMessage[] =>
  messages.flatMap((raw, index) => {
    if (typeof raw !== 'object' || raw === null) return []
    const message = raw as { id?: unknown; entryId?: unknown; role?: unknown; content?: unknown }
    const id = typeof message.id === 'string' ? message.id : typeof message.entryId === 'string' ? message.entryId : `m-${index}`
    const role = message.role === 'user' ? 'user' : 'assistant'
    const content =
      typeof message.content === 'string'
        ? message.content
        : Array.isArray(message.content)
          ? message.content
              .map((part) =>
                typeof part === 'object' && part !== null && typeof (part as { text?: unknown }).text === 'string'
                  ? (part as { text: string }).text
                  : '',
              )
              .join('')
          : ''
    return content.trim().length > 0 ? [{ id, role: role as HistoryMessage['role'], content }] : []
  })

export function useWorkspace(): Workspace {
  const [status, setStatus] = useState<Status>('boot')
  const [presentationReady, setPresentationReady] = useState(false)
  const [source, setSource] = useState<Source>({ kind: sample ? 'fixture' : 'disconnected' })
  const [error, setError] = useState<string | null>(null)
  const [projects, setProjects] = useState<Project[]>(sample?.projects ?? [])
  const [sessions, setSessions] = useState<Session[]>(sample?.sessions ?? [])
  const [history, setHistory] = useState<HistoryMessage[]>(sample?.history ?? [])
  const [blocks, setBlocks] = useState<Block[]>(sample?.blocks ?? [])
  const [selected, setSelected] = useState({ project: sample?.sessions[0]?.projectId ?? '', session: sample?.sessions[0]?.id ?? '' })
  const connection = useRef<NodeConnection | null>(null)
  const epoch = useRef(0)
  const sessionTicket = useRef(0)
  const [connectionId, setConnectionId] = useState(0)
  const [creation, setCreation] = useState<Workspace['creation']>({ busy: false, uncertain: false, message: null })
  type Creation = { conn: NodeConnection; epoch: number; ticket: number; kind: 'project' | 'session'; id?: string; reading?: boolean }
  const creationRequest = useRef<Creation | null>(null)
  const pendingOrigin = useRef<string>('')
  const presenter = useRef<TranscriptPresenter | null>(null)
  const historyRef = useRef<HistoryMessage[]>(sample?.history ?? [])
  const subscription = useRef<number | null>(null)
  const [liveView, setLiveView] = useState<LiveView | null>(null)
  const [runtime, setRuntime] = useState<NodeRuntime | null>(null)
  const [models, setModels] = useState<NodeModel[]>([])
  const [thinkingLevels, setThinkingLevels] = useState<string[]>([])
  const [busy, setBusy] = useState(false)
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
  const [filePath, setFilePath] = useState<string | null>(null)
  const [fileBody, setFileBody] = useState<string | null>(null)
  const [fileDiff, setFileDiff] = useState<string | null>(null)
  const [sendError, setSendError] = useState<string | null>(null)
  const sessionIdRef = useRef<string>('')

  useEffect(() => {
    let active = true
    void initViewModel().then(() => {
      if (!active) return
      presenter.current = new TranscriptPresenter()
      setPresentationReady(true)
      if (epoch.current === 0) setStatus(sample ? 'fixture' : 'disconnected')
    }).catch(cause => {
      if (!active) return
      setError(`显示组件加载失败：${String(cause)}`)
      setStatus('error')
    })
    return () => { active = false }
  }, [])

  const stopLive = useCallback(() => {
    if (subscription.current !== null && connection.current) {
      connection.current.stop(subscription.current)
    }
    subscription.current = null
  }, [])

  const clearSession = useCallback((project: string) => {
    stopLive()
    sessionTicket.current++
    sessionIdRef.current = ''
    historyRef.current = []
    setSelected({ project, session: '' })
    setHistory([])
    setBlocks([])
    setLiveView(null)
    setRuntime(null)
    setModels([])
    setThinkingLevels([])
    setQueue({ items: [], count: 0 })
    setDialogs([])
    setBusy(false)
    setSendError(null)
    setChanges([])
    setFiles([])
    setFilePath(null)
    setFileBody(null)
    setFileDiff(null)
    setResources({ rows: [], counts: [0, 0, 0] })
  }, [stopLive])

  const loadSession = useCallback(async (conn: NodeConnection, sessionId: string, projectId: string) => {
    if (conn !== connection.current) return
    clearSession(projectId)
    const generation = epoch.current
    const ticket = sessionTicket.current
    const current = () => connection.current === conn && epoch.current === generation && sessionTicket.current === ticket
    setSelected({ project: projectId, session: sessionId })
    setStatus('connecting')
    try {
      await initViewModel()
      if (!current()) return
      const page = await conn.messages(sessionId)
      if (!current()) return
      const messages = asHistory(page.messages)
      historyRef.current = messages
      setHistory(messages)
      setBlocks([])
      setLiveView(null)
      setSelected({ project: projectId, session: sessionId })
      sessionIdRef.current = sessionId
      setStatus('ready')
      void conn
        .queue(sessionId)
        .then(value => { if (current()) setQueue(value) })
        .catch(() => { if (current()) setQueue({ items: [], count: 0 }) })
      // Runtime state, model catalogue and thinking levels come from the node.
      void Promise.all([conn.runtimeState(sessionId), conn.models(sessionId), conn.thinkingLevels(sessionId)])
        .then(([state, catalogue, levels]) => {
          if (!current()) return
          setRuntime(state)
          setModels(catalogue)
          setThinkingLevels(levels)
        })
        .catch(() => {
          // Unknown observations stay empty; never retain another session's catalogue.
        })
      // Live events replay through the Rust projection; the presenter dedupes
      // reconnects, so subscribing from 0 is safe.
      const scope = `${generation}:${conn.origin}/${projectId}/${sessionId}`
      subscription.current = conn.subscribe(sessionId, 0, (event) => {
        if (!current()) return
        // Dialogs are data, not display rules: collect them as they arrive and
        // drop them when the node says they closed.
        const wire = event as { event_type?: string; data?: Record<string, unknown> }
        const ui = (wire.data?.ui ?? wire.data) as
          | { purpose?: string; question?: string; message?: string; options?: { label?: string; value?: string }[] }
          | undefined
        const requestId = (wire.data?.requestId ?? (wire.data as { id?: string })?.id) as string | undefined
        if (wire.event_type === 'ui_dialog_request' && requestId) {
          const options = (ui?.options ?? [])
            .map((option) => option.label ?? option.value ?? '')
            .filter((label) => label.length > 0)
          setDialogs((current) =>
            current.some((dialog) => dialog.requestId === requestId)
              ? current
              : [
                  ...current,
                  {
                    requestId,
                    purpose: ui?.purpose ?? null,
                    question: ui?.question ?? ui?.message ?? null,
                    options: options.length > 0 ? options : ['允许一次', '拒绝'],
                  },
                ],
          )
        } else if (wire.event_type === 'ui_dialog_closed' && requestId) {
          setDialogs((current) => current.filter((dialog) => dialog.requestId !== requestId))
        }
        const view = presenter.current?.applyEvents(scope, historyRef.current, [event])
        if (!view) return
        setLiveView(view)
        setBlocks(view.blocks as Block[])
      })
    } catch (cause) {
      if (!current()) return
      setError(String(cause))
      setStatus('error')
    }
  }, [clearSession])

  const finishConnect = useCallback(async (conn: NodeConnection, generation: number) => {
    const [nodeProjects, nodeSessions] = await Promise.all([conn.projects(), conn.sessions()])
    if (connection.current !== conn || epoch.current !== generation) return
    const mappedSessions = nodeSessions.map(asSession)
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

  const openNode = useCallback(
    async (origin: string) => {
      const generation = ++epoch.current
      setConnectionId(generation)
      clearSession('')
      setProjects([])
      setSessions([])
      setSource({ kind: 'disconnected' })
      connection.current = null
      creationRequest.current = null
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

  const send = useCallback(
    async (text: string) => {
      const conn = connection.current
      const sessionId = sessionIdRef.current
      if (!conn || !sessionId || text.trim().length === 0) return false
      setBusy(true)
      setSendError(null)
      try {
        const reply = await conn.prompt(sessionId, text)
        if (!reply.accepted) {
          setSendError(reply.error ?? '节点拒绝了这次发送')
          return false
        }
        return true
      } catch (cause) {
        setSendError(String(cause))
        return false
      } finally {
        setBusy(false)
      }
    },
    [],
  )

  const refreshQueue = useCallback(async () => {
    const conn = connection.current
    const sessionId = sessionIdRef.current
    if (!conn || !sessionId) return
    try {
      setQueue(await conn.queue(sessionId))
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [])

  const queueMessage = useCallback(
    async (text: string) => {
      const conn = connection.current
      const sessionId = sessionIdRef.current
      if (!conn || !sessionId || text.trim().length === 0) return false
      try {
        await conn.queueMessage(sessionId, text)
        await refreshQueue()
        return true
      } catch (cause) {
        setSendError(String(cause))
        return false
      }
    },
    [refreshQueue],
  )

  const cancelQueued = useCallback(
    async (entryId: string) => {
      const conn = connection.current
      const sessionId = sessionIdRef.current
      if (!conn || !sessionId) return
      try {
        await conn.cancelQueued(sessionId, entryId)
        await refreshQueue()
      } catch (cause) {
        setSendError(String(cause))
      }
    },
    [refreshQueue],
  )

  const clearQueue = useCallback(async () => {
    const conn = connection.current
    const sessionId = sessionIdRef.current
    if (!conn || !sessionId) return
    try {
      setQueue(await conn.clearQueue(sessionId))
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [])

  const issueAuthorization = useCallback(async () => {
    const conn = connection.current
    if (!conn) return
    setAuthorizationError(null)
    try {
      const issued = await conn.issueQrAuthorization()
      // The payload envelope is produced by the Rust codec, not by the UI.
      setAuthorization({ payload: sealTicket(issued.ticket), expiresAt: issued.expiresAt })
    } catch (cause) {
      setAuthorizationError(String(cause))
    }
  }, [])

  const loadNodeInfo = useCallback(async () => {
    const conn = connection.current
    if (!conn) return
    try {
      const info = await conn.nodeInfo()
      setNodeInfo({
        version: info.version,
        service: info.service,
        protocol: info.protocol,
        nodeId: info.nodeId,
      })
    } catch {
      // Health is informational; a failure only hides the version row.
    }
  }, [])

  const loadProviders = useCallback(async () => {
    const conn = connection.current
    if (!conn) return
    setProviderError(null)
    try {
      const catalog = await conn.providerCatalog()
      setProviders(catalog.providers)
      setProviderRevision(catalog.revision)
    } catch (cause) {
      setProviderError(String(cause))
    }
  }, [])

  const answerDialog = useCallback(async (requestId: string, values: string[]) => {
    const conn = connection.current
    const sessionId = sessionIdRef.current
    if (!conn || !sessionId) return
    try {
      await conn.dialogResponse(sessionId, requestId, values)
      setDialogs((current) => current.filter((dialog) => dialog.requestId !== requestId))
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [])

  const saveProviderKey = useCallback(
    async (id: string, key: string) => {
      const conn = connection.current
      if (!conn || !providerRevision) {
        setProviderError('还没有读取到 provider 目录，无法安全写入')
        return false
      }
      setProviderError(null)
      setProviderNotice(null)
    setNodeInfo(null)
    setDialogs([])
      try {
        const receipt = await conn.setProviderKey(id, key, providerRevision)
        // The receipt carries the authoritative catalogue; use it as the truth.
        setProviders(receipt.providers)
        setProviderRevision(receipt.revision)
        setProviderNotice(
          receipt.requiresRestart ? '已保存，节点需要重启后生效' : '已保存并生效',
        )
        return true
      } catch (cause) {
        setProviderError(String(cause))
        return false
      }
    },
    [providerRevision],
  )

  const loadResources = useCallback(async () => {
    const conn = connection.current
    if (!conn || !selected.project) return
    try {
      const catalog = await conn.nativeCatalog(selected.project)
      setResources(resourceRows(catalog, selected.project))
      const packages = await conn.packageCatalog(selected.project)
      setOffers(packages.packages ?? [])
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [selected.project])

  const reviewOffer = useCallback(
    async (offer: unknown) => {
      const conn = connection.current
      if (!conn || !selected.project) return
      setInstallError(null)
      try {
        const review = await conn.packageBundle(selected.project, offer)
        setBundle(review)
        setReviewedOffer(offer)
      } catch (cause) {
        setInstallError(String(cause))
      }
    },
    [selected.project],
  )

  const cancelReview = useCallback(() => {
    setBundle(null)
    setReviewedOffer(null)
    setInstallError(null)
  }, [])

  const confirmInstall = useCallback(
    async (scope: 'global' | 'project') => {
      const conn = connection.current
      if (!conn || !selected.project || !reviewedOffer) return
      setInstalling(true)
      setInstallError(null)
      try {
        await conn.installPackage(selected.project, reviewedOffer, scope)
        setBundle(null)
        setReviewedOffer(null)
        // Read the catalogue back: the receipt is not proof of installation.
        await loadResources()
      } catch (cause) {
        setInstallError(String(cause))
      } finally {
        setInstalling(false)
      }
    },
    [selected.project, reviewedOffer, loadResources],
  )

  const loadChanges = useCallback(async () => {
    const conn = connection.current
    if (!conn || !selected.project) return
    try {
      const status = await conn.gitStatus(selected.project)
      setChanges(status.files.slice(0, 200))
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [selected.project])

  const loadFiles = useCallback(
    async (path = '') => {
      const conn = connection.current
      if (!conn || !selected.project) return
      try {
        const listing = await conn.files(selected.project, path)
        setFiles(listing.entries)
      } catch (cause) {
        setSendError(String(cause))
      }
    },
    [selected.project],
  )

  const openChange = useCallback(
    async (path: string, staged = false) => {
      const conn = connection.current
      if (!conn || !selected.project) return
      try {
        const diff = await conn.gitDiff(selected.project, path, staged)
        setFilePath(path)
        setFileDiff(diff.patch)
        setFileBody(null)
      } catch (cause) {
        setSendError(String(cause))
      }
    },
    [selected.project],
  )

  const openFile = useCallback(
    async (path: string) => {
      const conn = connection.current
      if (!conn || !selected.project) return
      try {
        const file = await conn.file(selected.project, path)
        setFilePath(path)
        setFileBody(file.content)
        setFileDiff(null)
      } catch (cause) {
        setSendError(String(cause))
      }
    },
    [selected.project],
  )

  const listDir = useCallback(
    async (path: string) => {
      const conn = connection.current
      if (!conn || !selected.project) return []
      const listing = await conn.files(selected.project, path)
      return listing.entries
    },
    [selected.project],
  )

  const abort = useCallback(async () => {
    const conn = connection.current
    const sessionId = sessionIdRef.current
    if (!conn || !sessionId) return
    try {
      await conn.abort(sessionId)
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [])

  const setModel = useCallback(async (model: NodeModel) => {
    const conn = connection.current
    const sessionId = sessionIdRef.current
    if (!conn || !sessionId) return
    try {
      setRuntime(await conn.setModel(sessionId, model.id, model.provider))
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [])

  const setThinking = useCallback(async (level: string) => {
    const conn = connection.current
    const sessionId = sessionIdRef.current
    if (!conn || !sessionId) return
    try {
      setRuntime(await conn.setThinking(sessionId, level))
    } catch (cause) {
      setSendError(String(cause))
    }
  }, [])

  const disconnect = useCallback(() => {
    stopLive()
    setConnectionId(++epoch.current)
    sessionTicket.current++
    creationRequest.current = null
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
    setFilePath(null)
    setFileBody(null)
    setFileDiff(null)
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
    setSelected({ project: '', session: '' })
    setBusy(false)
    setError(null)
    setStatus('disconnected')
  }, [stopLive])

  // A committed create is not inserted optimistically. Reconcile by its exact
  // node-returned ID; an unknown outcome stays locked and is never re-posted.
  const readCreation = useCallback(async (request: Creation): Promise<boolean> => {
    if (request.reading) return false
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
      const verified = request.kind === 'project' ? nodeProjects.some(item => item.id === request.id) : mapped.some(item => item.id === request.id)
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

  const create = useCallback(async (kind: 'project' | 'session', name = '', path = ''): Promise<boolean> => {
    const conn = connection.current
    if (!conn || source.kind !== 'live' || status !== 'ready' || creationRequest.current) return false
    if (kind === 'session' && !projects.some(project => project.id === selected.project)) return false
    if (kind === 'project' && (!name.trim() || !path.trim())) {
      setCreation({ busy: false, uncertain: false, message: '请填写项目名称和目标节点上的目录路径。' })
      return false
    }
    const request: Creation = { conn, epoch: epoch.current, ticket: sessionTicket.current, kind }
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
      return await readCreation(request)
    } catch (cause) {
      if (current()) setCreation({ busy: false, uncertain: true, message: `创建结果未确认：${String(cause)}。勿重复提交，请先核对节点。` })
      return false
    }
  }, [source, status, projects, selected.project, readCreation])

  const createProject = useCallback((name: string, path: string) => create('project', name, path), [create])
  const createSession = useCallback(() => create('session'), [create])
  const recheckCreation = useCallback(async () => {
    const request = creationRequest.current
    if (!request || creation.busy) return false
    setCreation({ busy: true, uncertain: true, message: '正在核对列表；不会重新发送创建请求。' })
    return readCreation(request)
  }, [creation.busy, readCreation])

  const selectProject = useCallback(
    (id: string) => {
      const next = sessions.find((session) => session.projectId === id && !session.archived)
      if (!next) {
        clearSession(id)
        if (source.kind === 'live') setStatus('ready')
        return
      }
      void selectSession(next.id)
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessions, source, clearSession],
  )

  const selectSession = useCallback(
    async (id: string) => {
      const session = sessions.find((entry) => entry.id === id)
      if (!session) return
      if (source.kind === 'live' && connection.current) {
        await loadSession(connection.current, session.id, session.projectId)
        return
      }
      setSelected({ project: session.projectId, session: session.id })
    },
    [sessions, source, loadSession],
  )

  const rows = useMemo<RowRef[]>(() => {
    if (!presentationReady) return []
    if (liveView && source.kind === 'live') return liveView.rows
    const instance = presenter.current
    if (!instance || (status !== 'fixture' && status !== 'ready')) return []
    const nodeScope = source.kind === 'live' ? `${connectionId}:${source.origin}` : 'fixture'
    return instance.rows(`${nodeScope}/${selected.project}/${selected.session || 'none'}`, history, blocks)
  }, [presentationReady, liveView, source, connectionId, status, selected, history, blocks])

  return {
    presentationReady,
    status,
    connectionId,
    creation,
    createProject,
    createSession,
    recheckCreation,
    source,
    error,
    projects,
    sessions,
    history,
    blocks,
    selected,
    rows,
    messageCount: history.length,
    runtime,
    models,
    thinkingLevels,
    busy,
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
    filePath,
    fileBody,
    fileDiff,
    loadChanges,
    loadFiles,
    openChange,
    openFile,
    listDir,
    refreshQueue,
    queueMessage,
    cancelQueued,
    clearQueue,
    send,
    abort,
    setModel,
    setThinking,
    connect,
    submitpassword,
    disconnect,
    selectProject,
    selectSession: (id: string) => void selectSession(id),
  }
}
