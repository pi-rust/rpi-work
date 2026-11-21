/**
 * Workspace state for the prototype.
 *
 * Two sources: the fixture (default, no node) and a live node reached through
 * the wasm `work-client` binding. Both feed the same presenter and the same
 * components, so layout never branches on the source.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import * as fixture from '@/data'
import { NodeConnection, type NodeSession } from '@/lib/node'
import { initViewModel, TranscriptPresenter, type RowRef } from '@/lib/viewmodel'
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

  useEffect(() => {
    void initViewModel().then(() => {
      presenter.current = new TranscriptPresenter()
      setStatus('fixture')
    })
  }, [])

  const loadSession = useCallback(async (conn: NodeConnection, sessionId: string, projectId: string) => {
    setStatus('connecting')
    try {
      const page = await conn.messages(sessionId)
      setHistory(asHistory(page.messages))
      setBlocks([])
      setSelected({ project: projectId, session: sessionId })
      setStatus('ready')
    } catch (cause) {
      setError(String(cause))
      setStatus('error')
    }
  }, [])

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

  const disconnect = useCallback(() => {
    connection.current = null
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
    const instance = presenter.current
    if (!instance || (status !== 'fixture' && status !== 'ready')) return []
    return instance.rows(`${selected.project}/${selected.session || 'none'}`, history, blocks)
  }, [status, selected, history, blocks])

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
    connect,
    submitpassword,
    disconnect,
    selectProject,
    selectSession: (id: string) => void selectSession(id),
  }
}
