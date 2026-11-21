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
