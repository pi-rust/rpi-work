/**
 * Workspace state for the prototype.
 *
 * Two sources: the fixture (default, no node) and a live node reached through
 * the wasm `work-client` binding. Both feed the same presenter and the same
 * components, so layout never branches on the source.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import * as fixture from '@/data'
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
import type { Block, HistoryMessage, Project, Session } from '@/data'

export type Source = { kind: 'fixture' } | { kind: 'live'; origin: string }
export type Status = 'boot' | 'fixture' | 'connecting' | 'auth-required' | 'ready' | 'error'

export type Workspace = {
  status: Status
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
  const [source, setSource] = useState<Source>({ kind: 'fixture' })
  const [error, setError] = useState<string | null>(null)
  const [projects, setProjects] = useState<Project[]>(fixture.projects)
  const [sessions, setSessions] = useState<Session[]>(fixture.sessions)
  const [history, setHistory] = useState<HistoryMessage[]>(fixture.history)
  const [blocks, setBlocks] = useState<Block[]>(fixture.blocks)
  const [selected, setSelected] = useState({ project: 'rpi-work', session: 'dark' })
  const connection = useRef<NodeConnection | null>(null)
  const pendingOrigin = useRef<string>('')
  const presenter = useRef<TranscriptPresenter | null>(null)
  const historyRef = useRef<HistoryMessage[]>(fixture.history)
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
    void initViewModel().then(() => {
      presenter.current = new TranscriptPresenter()
      setStatus('fixture')
    })
  }, [])

  const stopLive = useCallback(() => {
    if (subscription.current !== null && connection.current) {
      connection.current.stop(subscription.current)
    }
    subscription.current = null
  }, [])

  const loadSession = useCallback(async (conn: NodeConnection, sessionId: string, projectId: string) => {
    stopLive()
    setStatus('connecting')
    try {
      const page = await conn.messages(sessionId)
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
        .then(setQueue)
        .catch(() => setQueue({ items: [], count: 0 }))
      // Runtime state, model catalogue and thinking levels come from the node.
      void Promise.all([conn.runtimeState(sessionId), conn.models(sessionId), conn.thinkingLevels(sessionId)])
        .then(([state, catalogue, levels]) => {
          setRuntime(state)
          setModels(catalogue)
          setThinkingLevels(levels)
        })
        .catch(() => {
          // Older nodes may not expose the catalogue; the header keeps the defaults.
        })
      // Live events replay through the Rust projection; the presenter dedupes
      // reconnects, so subscribing from 0 is safe.
      const scope = `${projectId}/${sessionId}`
      subscription.current = conn.subscribe(sessionId, 0, (event) => {
        const view = presenter.current?.applyEvents(scope, historyRef.current, [event])
        if (!view) return
        setLiveView(view)
        setBlocks(view.blocks as Block[])
      })
    } catch (cause) {
      setError(String(cause))
      setStatus('error')
    }
  }, [stopLive])

  const openNode = useCallback(
    async (origin: string) => {
      setError(null)
      setStatus('connecting')
      pendingOrigin.current = origin
      try {
        const conn = await NodeConnection.open(origin)
        connection.current = conn
        const auth = await conn.auth()
        if (auth.required && !auth.authenticated) {
          setStatus('auth-required')
          return
        }
        await finishConnect(conn)
      } catch (cause) {
        setError(String(cause))
        setStatus('error')
      }
    },
    // finishConnect is stable through useCallback below
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  )

  const finishConnect = useCallback(async (conn: NodeConnection) => {
    const [nodeProjects, nodeSessions] = await Promise.all([conn.projects(), conn.sessions()])
    const mappedProjects = nodeProjects.length > 0 ? nodeProjects : [{ id: '', name: '(空节点)', path: '' }]
    const mappedSessions = nodeSessions.map(asSession)
    setSource({ kind: 'live', origin: conn.origin })
    setProjects(mappedProjects.map(({ id, name }) => ({ id, name })))
    setSessions(mappedSessions)
    const first = mappedSessions[0]
    if (first) {
      await loadSession(conn, first.id, first.projectId)
    } else {
      setHistory([])
      setBlocks([])
      setSelected({ project: mappedProjects[0]?.id ?? '', session: '' })
      setStatus('ready')
    }
  }, [loadSession])

  const connect = useCallback(
    async (origin: string) => {
      await openNode(origin)
    },
    [openNode],
  )

  const submitpassword = useCallback(
    async (password: string) => {
      const conn = connection.current
      if (!conn) return
      setStatus('connecting')
      try {
        await conn.login(password)
        await openNode(conn.origin)
      } catch (cause) {
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
    historyRef.current = fixture.history
    setSource({ kind: 'fixture' })
    setProjects(fixture.projects)
    setSessions(fixture.sessions)
    setHistory(fixture.history)
    setBlocks(fixture.blocks)
    setSelected({ project: 'rpi-work', session: 'dark' })
    setError(null)
    setStatus('fixture')
  }, [])

  const selectProject = useCallback(
    (id: string) => {
      const next = sessions.find((session) => session.projectId === id)
      if (!next) {
        setSelected({ project: id, session: '' })
        setHistory([])
        setBlocks([])
        return
      }
      void selectSession(next.id)
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessions, source],
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
    if (liveView && source.kind === 'live') return liveView.rows
    const instance = presenter.current
    if (!instance || (status !== 'fixture' && status !== 'ready')) return []
    return instance.rows(`${selected.project}/${selected.session || 'none'}`, history, blocks)
  }, [liveView, source, status, selected, history, blocks])

  return {
    status,
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
