/**
 * Typed wrapper around the `work-client` wasm binding.
 *
 * The node protocol (paths, command envelopes, error shapes) stays in Rust;
 * this module only adapts promises to TypeScript types.
 */
import clientWasmUrl from '../pkg/work_client_bg.wasm?url'
import initClient, { NodeClient, SessionClient } from '../pkg/work_client.js'
import type { LiveView } from '@/lib/viewmodel'

export type NodeProject = { id: string; name: string; path: string }
export type CreationReply<T> =
  | { outcome: 'created'; value: T }
  | { outcome: 'rejected' | 'unknown'; error: string }

export type ArchiveReply =
  | { outcome: 'saved'; value: { sessionId: string; archived: boolean; pinned: boolean } }
  | { outcome: 'rejected' | 'unknown'; error: string }

export type NodeSession = {
  sessionId: string
  projectId: string | null
  title: string | null
  status: string
  updatedAt: number
  lastSeq: number
  archived: boolean
  pinned: boolean
}

export type NodeAuth = { required: boolean; authenticated: boolean }
export type NodeModel = {
  id: string
  name: string
  provider: string
  reasoning: boolean
  contextWindow: number | null
}
export type NodeRuntime = {
  modelId: string | null
  provider: string | null
  thinkingLevel: string | null
  context?: { tokens?: number | null; window?: number | null; percent?: number | null; threshold?: number | null }
  stats?: Record<string, unknown> | null
  queue?: { count?: number } | null
}
export type NodeQueueItem = {
  entryId: string
  queue: string
  text: string
  status: string | null
}
export type NodeQueue = { items: NodeQueueItem[]; count: number }
export type NodeFileEntry = { name: string; path: string; kind: string; size: number }
export type NodeProvider = {
  id: string
  definition: { name?: string; models?: { id: string; name?: string }[] } | null
  configured: boolean
  configKeyPresent: boolean
  storedCredential: string | null
  diagnostic: string | null
}
export type NodeProviderCatalog = { revision: string; providers: NodeProvider[] }
export type NodeChangedFile = {
  path: string
  originalPath: string | null
  status: string
  staged: boolean
  unstaged: boolean
}
export type NodeGitStatus = {
  isRepository: boolean
  branch: string | null
  files: NodeChangedFile[]
}
export type NodeMessages = {
  messages: unknown[]
  firstId: string | null
  lastId: string | null
  hasMoreBefore: boolean
}

let ready: Promise<void> | null = null

export function initNodeClient(): Promise<void> {
  ready ??= initClient(clientWasmUrl).then(() => undefined)
  return ready
}

const parse = <T,>(value: string): T => JSON.parse(value) as T

export type SessionControls = {
  busy: boolean; online: boolean; send: boolean; queue: boolean; queueRead: boolean
  cancelQueued: boolean; clearQueue: boolean; abort: boolean; reply: boolean
  model: boolean; thinking: boolean; configurationLocked: boolean; reason: string
}
export type SessionDialog = {
  requestId: string; purpose: string | null; question: string | null; replyEnabled: boolean
  options: { label: string; value: string | null }[]
}
export type SessionRecovery = { canCheck: boolean; checking: boolean; count: number; unreceipted: boolean; message: string | null }
export type SessionView = LiveView & {
  recovery: SessionRecovery
  scopeEpoch: number; messages: unknown[]; controls: SessionControls; dialogs: SessionDialog[]
  runtime: NodeRuntime | null; models: NodeModel[]; thinkingLevels: string[]; error: string | null
}

/** Lifetime-only adapter: all admission and execution facts stay in Rust. */
export class BoundSession {
  private readonly client: SessionClient
  constructor(client: SessionClient) { this.client = client }
  close() { this.client.close(); this.client.free() }
  async recheck(): Promise<SessionRecovery> { return parse(await this.client.recheck()) }
  async prompt(text: string): Promise<{ accepted: boolean; error: string | null }> { return parse(await this.client.prompt(text)) }
  async abort(): Promise<unknown> { return parse(await this.client.abort()) }
  async queue(): Promise<NodeQueue> { return parse(await this.client.queue()) }
  async queueMessage(text: string): Promise<{ accepted: boolean | null }> { return parse(await this.client.queue_message(text, 'followUp')) }
  async cancelQueued(id: string): Promise<unknown> { return parse(await this.client.cancel_queued(id)) }
  async clearQueue(): Promise<NodeQueue> { return parse(await this.client.clear_queue()) }
  async setModel(id: string, provider: string): Promise<NodeRuntime> { return parse(await this.client.set_model(id, provider)) }
  async setThinking(level: string): Promise<NodeRuntime> { return parse(await this.client.set_thinking(level)) }
  async answer(id: string, values: string[]): Promise<unknown> { return parse(await this.client.dialog_response(id, JSON.stringify(values))) }
}

export class NodeConnection {
  private readonly client: NodeClient
  readonly origin: string

  private constructor(client: NodeClient, origin: string) {
    this.client = client
    this.origin = origin
  }

  static async open(origin: string): Promise<NodeConnection> {
    await initNodeClient()
    const client = new NodeClient(origin)
    return new NodeConnection(client, client.origin)
  }

  async bindSession(id: string, update: (view: SessionView) => void): Promise<BoundSession> {
    return new BoundSession(await this.client.bind_session(id, (text: string) => update(parse<SessionView>(text))))
  }

  async auth(): Promise<NodeAuth> {
    return parse<NodeAuth>(await this.client.auth())
  }

  async login(password: string): Promise<NodeAuth> {
    return parse<NodeAuth>(await this.client.login(password, false))
  }

  async projects(): Promise<NodeProject[]> {
    return parse<NodeProject[]>(await this.client.projects())
  }

  async sessions(): Promise<NodeSession[]> {
    return parse<NodeSession[]>(await this.client.sessions())
  }

  async supportsSessionPreferences(): Promise<boolean> {
    return this.client.supports_session_preferences()
  }

  async setSessionArchived(id: string, archived: boolean): Promise<ArchiveReply> {
    return parse(await this.client.set_session_archived(id, archived))
  }

  async createProject(name: string, path: string): Promise<CreationReply<NodeProject>> {
    return parse(await this.client.create_project(name, path))
  }

  async createSession(projectId: string): Promise<CreationReply<string>> {
    return parse(await this.client.create_session(projectId))
  }

  async messages(sessionId: string, limit = 60): Promise<NodeMessages> {
    return parse<NodeMessages>(await this.client.messages(sessionId, limit))
  }

  async prompt(sessionId: string, message: string): Promise<{ accepted: boolean; error: string | null }> {
    return parse(await this.client.prompt(sessionId, message))
  }

  async abort(sessionId: string): Promise<{ accepted: boolean; error: string | null }> {
    return parse(await this.client.abort(sessionId))
  }

  async runtimeState(sessionId: string): Promise<NodeRuntime> {
    return parse<NodeRuntime>(await this.client.runtime_state(sessionId))
  }

  async models(sessionId: string): Promise<NodeModel[]> {
    return parse<NodeModel[]>(await this.client.models(sessionId))
  }

  async thinkingLevels(sessionId: string): Promise<string[]> {
    return parse<string[]>(await this.client.thinking_levels(sessionId))
  }

  async setModel(sessionId: string, modelId: string, provider: string): Promise<NodeRuntime> {
    return parse<NodeRuntime>(await this.client.set_model(sessionId, modelId, provider))
  }

  async setThinking(sessionId: string, level: string): Promise<NodeRuntime> {
    return parse<NodeRuntime>(await this.client.set_thinking(sessionId, level))
  }

  async providerCatalog(): Promise<NodeProviderCatalog> {
    return parse<NodeProviderCatalog>(await this.client.provider_catalog())
  }

  async packageCatalog(projectId: string): Promise<{ packages: unknown[]; source: string }> {
    return parse(await this.client.package_catalog(projectId))
  }

  async packageBundle(projectId: string, offer: unknown): Promise<{
    manifest: { id: string; title: string; version: string; kind: string }
    sources: Record<string, string>
    sha256: string
  }> {
    return parse(await this.client.package_bundle(projectId, JSON.stringify(offer)))
  }

  async installPackage(projectId: string, offer: unknown, scope: 'global' | 'project'): Promise<unknown> {
    return parse(await this.client.install_package(projectId, JSON.stringify(offer), scope))
  }

  async dialogResponse(sessionId: string, requestId: string, values: string[]): Promise<void> {
    await this.client.dialog_response(sessionId, requestId, JSON.stringify(values))
  }

  async nodeInfo(): Promise<{
    ok: boolean
    service: string | null
    version: string | null
    protocol: string | null
    nodeId: string | null
  }> {
    return parse(await this.client.node_info())
  }

  async setProviderKey(
    id: string,
    key: string,
    expectedRevision: string,
  ): Promise<{ revision: string; providers: NodeProvider[]; requiresRestart: boolean }> {
    return parse(await this.client.set_provider_key(id, key, expectedRevision))
  }

  async issueQrAuthorization(): Promise<{ ticket: string; expiresAt: number; nodeId: string }> {
    return parse(await this.client.issue_qr_authorization())
  }

  /** Raw native catalog payload; the presenter projects it into rows. */
  async nativeCatalog(projectId: string): Promise<unknown> {
    return parse(await this.client.native_catalog(projectId))
  }

  async files(projectId: string, path = ''): Promise<{ path: string; entries: NodeFileEntry[] }> {
    return parse(await this.client.files(projectId, path))
  }

  async file(projectId: string, path: string): Promise<{ path: string; content: string }> {
    return parse(await this.client.file(projectId, path))
  }

  async gitStatus(projectId: string): Promise<NodeGitStatus> {
    return parse<NodeGitStatus>(await this.client.git_status(projectId))
  }

  async gitDiff(projectId: string, path: string, staged = false): Promise<{ path: string; patch: string }> {
    return parse(await this.client.git_diff(projectId, path, staged))
  }

  async queue(sessionId: string): Promise<NodeQueue> {
    return parse<NodeQueue>(await this.client.queue(sessionId))
  }

  async queueMessage(sessionId: string, message: string, deliverAs = 'followUp'): Promise<unknown> {
    return parse(await this.client.queue_message(sessionId, message, deliverAs))
  }

  async cancelQueued(sessionId: string, entryId: string): Promise<unknown> {
    return parse(await this.client.cancel_queued(sessionId, entryId))
  }

  async clearQueue(sessionId: string): Promise<NodeQueue> {
    return parse<NodeQueue>(await this.client.clear_queue(sessionId))
  }

  /** Live wire events; returns the subscription id used by `stop`. */
  subscribe(sessionId: string, after: number, onEvent: (event: unknown) => void): number {
    return this.client.subscribe(sessionId, after, (raw: unknown) => {
      // The Rust side sends one JSON string per event.
      let event: unknown = raw
      if (typeof raw === 'string') {
        try {
          event = JSON.parse(raw)
        } catch {
          return
        }
      }
      if (typeof event === 'object' && event !== null && 'error' in event) {
        console.error('live subscription error:', (event as { error: string }).error)
        return
      }
      onEvent(event)
    })
  }

  stop(subscription: number) {
    this.client.stop(subscription)
  }
}
