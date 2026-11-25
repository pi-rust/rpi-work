/**
 * Typed wrapper around the `work-viewmodel` wasm binding.
 *
 * The Rust crate is the single source of truth for transcript ordering, tool
 * labels, code tokens, metric formatting and theme tokens. React only lays out
 * what these functions return.
 */
import wasmUrl from '../pkg/work_viewmodel_bg.wasm?url'
import init, { resource_rows, seal_ticket,
  Presenter,
  context_reported,
  format_percent,
  format_tokens,
  diff_view,
  diff_feedback_draft,
  worktree_review_prompt,
  file_view,
  metrics_session,
  markdown_view,
  normalize_address,
  theme_presets,
  theme_tokens,
  tokenize_file,
  tool_icon,
  tool_is_read,
  tool_is_shell,
  tool_target,
} from '../pkg/work_viewmodel.js'

export type RowRef = { key: number; source: 'history' | 'projection'; index: number }
export type Token = { text: string; class: string }
export type FileLine = { number: number | null; kind: 'source' | 'meta' | 'add' | 'remove' | 'context' | 'plain'; tokens: Token[] }
export const fileView = (source: string, path: string, diff: boolean): FileLine[] => JSON.parse(file_view(source, path, diff)) as FileLine[]
export type DiffLine = Omit<FileLine, 'kind'> & { kind: FileLine['kind'] | 'hunk' | 'note'; old_number: number | null; new_number: number | null }
export type DiffView = { rows: DiffLine[]; structured: boolean; fallback_reason: 'limit' | 'invalid' | 'unsupported' | 'no_hunks' | null; gutter_digits: number; feedback_revision: string | null }
export const diffView = (source: string, path: string): DiffView => JSON.parse(diff_view(source, path)) as DiffView
export { worktree_review_prompt as worktreeReviewPrompt }
export type DraftFeedback = { path: string; staged: boolean; index: number; revision: string; location: string; comment: string; message: string }
export function diffFeedbackDraft(expected: string, fresh: string, path: string, staged: boolean, index: number, text: string): DraftFeedback | null {
  const result = diff_feedback_draft(expected, fresh, path, staged, index, text)
  return result ? JSON.parse(result) as DraftFeedback : null
}
export type MarkdownTag = 'div' | 'p' | 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6' | 'blockquote' | 'ul' | 'ol' | 'li' | 'em' | 'strong' | 'del' | 'code' | 'span' | 'br' | 'hr' | 'table' | 'thead' | 'tbody' | 'tr' | 'th' | 'td'
export type MarkdownNode =
  | { kind: 'text'; text: string }
  | { kind: 'element'; tag: MarkdownTag; children: MarkdownNode[]; start?: number }
  | { kind: 'link'; href: string | null; children: MarkdownNode[] }
  | { kind: 'image'; alt: string }
  | { kind: 'codeBlock'; code: string; language: string; tokens: Token[] }
  | { kind: 'task'; checked: boolean }
export type MarkdownView = { nodes: MarkdownNode[]; copyText: string; plain: boolean }
export const markdownView = (source: string): MarkdownView => JSON.parse(markdown_view(source)) as MarkdownView
export type ResourceRow = {
  reference: string
  name: string
  nativeName: string
  version: string
  kind: string
  kindLabel: string
  icon: string
  scope: 'global' | 'project'
  project: string
  installed: boolean
  origin: string
}
export type ResourceView = { rows: ResourceRow[]; counts: [number, number, number] }
export type MetricsView = {
  input: number | null
  output: number | null
  cachePercent: number | null
  cost: number | null
  requests: number | null
  retries: number | null
  speed: number | null
  inputText: string
  outputText: string
  cacheText: string
}
export type ContextView = {
  used: number | null
  window: number | null
  percent: number | null
  threshold: number | null
  percentText: string
}

let ready: Promise<void> | null = null

export function initViewModel(): Promise<void> {
  ready ??= init(wasmUrl).then(() => undefined)
  return ready
}

export type LiveView = { rows: RowRef[]; blocks: unknown[]; phase: string; settled: boolean; transcript?: ConversationView }
export type ActivityItem =
  | { kind: 'tool'; key: string; block: Omit<import('./types').ToolBlock, 'kind'> & { statusLabel: string } }
  | { kind: 'note'; key: string; label: string; text: string }
export type ConversationPart =
  | { kind: 'message'; key: string; readingKey?: string; forkEntryId?: string; ratingEntryId?: string; role: 'user' | 'assistant'; text: string }
  | { kind: 'activity'; key: string; label: string; collapsed: boolean; attention: boolean; items: ActivityItem[] }
export type ConversationView = { observation: string; observationTone: 'muted' | 'active' | 'success' | 'danger' | 'warning'; parts: ConversationPart[] }

export class TranscriptPresenter {
  private presenter = new Presenter()

  rows(scope: string, history: unknown[], blocks: unknown[]): RowRef[] {
    return JSON.parse(
      this.presenter.rows(scope, JSON.stringify(history), JSON.stringify(blocks)),
    ) as RowRef[]
  }

  conversation(scope: string, history: unknown[], blocks: unknown[]): ConversationView {
    return JSON.parse(this.presenter.conversation(scope, JSON.stringify(history), JSON.stringify(blocks))) as ConversationView
  }

  /** Replay live wire events through the Rust projection, then order rows. */
  applyEvents(scope: string, history: unknown[], events: unknown[]): LiveView {
    return JSON.parse(
      this.presenter.apply_events(scope, JSON.stringify(history), JSON.stringify(events)),
    ) as LiveView
  }

  forget(scope: string) {
    this.presenter.forget(scope)
  }
}

export function toolMeta(name: string, argumentsJson: string) {
  return {
    target: tool_target(name, argumentsJson),
    icon: tool_icon(name),
    isRead: tool_is_read(name),
    isShell: tool_is_shell(name),
  }
}

export const tokenizeFile = (source: string, path: string): Token[][] =>
  JSON.parse(tokenize_file(source, path)) as Token[][]

export const metricsFrom = (stats: unknown): MetricsView =>
  JSON.parse(metrics_session(JSON.stringify(stats))) as MetricsView

export const contextFrom = (context: unknown): ContextView =>
  JSON.parse(context_reported(JSON.stringify(context))) as ContextView

export const sealTicket = (ticket: string): string => seal_ticket(ticket)

export const resourceRows = (catalog: unknown, projectId: string): ResourceView =>
  JSON.parse(resource_rows(JSON.stringify(catalog ?? {}), projectId)) as ResourceView

export const themeTokens = (themeId: string, dark: boolean, compact = false) =>
  JSON.parse(theme_tokens(themeId, dark, compact)) as Record<string, string>

export const themePresets = () =>
  JSON.parse(theme_presets()) as { id: string; name: string; description: string }[]

export { format_tokens as formatTokens, format_percent as formatPercent, normalize_address as normalizeAddress }
