// Renderer-facing data shapes. Protocol and presentation rules remain in Rust.
export type Project = { id: string; name: string }
export type Session = { id: string; projectId: string; title: string; updatedAt: number; pinned?: boolean; archived?: boolean }
export type HistoryMessage = { id: string; role: 'user' | 'assistant'; content: string }
export type TextBlock = { kind: 'text'; id: string; text: string }
export type ToolBlock = { kind: 'tool'; id: string; name: string; arguments: string; output: string; status: 'completed' | 'running' | 'failed' | 'cancelled' | 'unknown'; statusLabel?: string }
export type Block = TextBlock | ToolBlock
export type RunStep = { id: string; title: string; status: 'done' | 'running' | 'pending'; at: string }
export type ToolDetail = { durationMs: number; language: string }
export type WorkspaceSample = {
  projects: Project[]
  sessions: Session[]
  history: HistoryMessage[]
  blocks: Block[]
  toolMeta: Record<string, ToolDetail>
  statsPayload: unknown
  contextPayload: unknown
  runSteps: RunStep[]
  node: { model: string; thinking: string }
}
