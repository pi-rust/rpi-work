/**
 * Typed wrapper around the `work-client` wasm binding.
 *
 * The node protocol (paths, command envelopes, error shapes) stays in Rust;
 * this module only adapts promises to TypeScript types.
 */
import clientWasmUrl from '../pkg/work_client_bg.wasm?url'
import initClient, { NodeClient } from '../pkg/work_client.js'

export type NodeProject = { id: string; name: string; path: string }

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
