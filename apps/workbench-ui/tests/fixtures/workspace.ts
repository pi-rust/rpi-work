/**
 * Fixture content for the ReUI/Codex layout prototype.
 *
 * The shapes match what the wasm presenter consumes: canonical history plus an
 * append-only run projection. Ordering, tool labels and metric formatting come
 * from `work-viewmodel` through `@/lib/viewmodel`, not from this file.
 */

import type { Project, Session, HistoryMessage, Block, RunStep, WorkspaceSample } from '../../src/lib/types'

export const projects: Project[] = [
  { id: 'rpi-work', name: 'rpi-work' },
  { id: 'pi-rust', name: 'pi-rust' },
]

export const sessions: Session[] = [
  { id: 'dark', projectId: 'rpi-work', title: '为工作台添加深色模式', updatedAt: 1792910000000 },
  { id: 'architecture', projectId: 'rpi-work', title: '梳理 Agent 架构', updatedAt: 1792900000000 },
  { id: 'mobile', projectId: 'rpi-work', title: '检查手机端布局', updatedAt: 1792890000000 },
  { id: 'rpc', projectId: 'pi-rust', title: '验证 RPC 事件协议', updatedAt: 1792880000000 },
]

/** Canonical history exactly as the node returns it. */
export const history: HistoryMessage[] = [
  { id: 'user-1', role: 'user', content: '帮我为工作台增加一个深色主题，并保持手机端可用。' },
]

/** Run projection blocks; the display order is decided in Rust. */
export const blocks: Block[] = [
  {
    kind: 'text',
    id: 'answer-1',
    text: '主题切换已经准备好，接下来运行组件测试。深色模式复用同一套 token，手机端沿用 320px 断点。',
  },
  {
    kind: 'tool',
    id: 'read-1',
    name: 'read',
    arguments: JSON.stringify({ path: 'src/theme.rs' }),
    output:
      'pub fn theme() -> Tokens {\n    let light = Tokens::codex(false);\n    let dark = Tokens::codex(true);\n    Tokens { light, dark }\n}',
    status: 'completed',
  },
  {
    kind: 'tool',
    id: 'bash-1',
    name: 'bash',
    arguments: JSON.stringify({ command: 'cargo test -p work-ui --lib' }),
    output: 'running 12 tests\ntest result: ok. 12 passed; 0 failed',
    status: 'completed',
  },
]

/** Render-only metadata keyed by block id. */
export const toolMeta: Record<string, { durationMs: number; language: string }> = {
  'read-1': { durationMs: 12, language: 'rust' },
  'bash-1': { durationMs: 1840, language: 'shell' },
}

export const statsPayload = {
  uncachedTokens: 8000,
  cacheReadTokens: 24000,
  cacheWriteTokens: 4800,
  outputTokens: 4800,
  costTotal: 0.0184,
  requests: 3,
}

export const contextPayload = { tokens: 38000, window: 100000, percent: 38, threshold: 84000 }

export const runSteps: RunStep[] = [
  { id: 'step-plan', title: '读取主题契约', status: 'done', at: '16:21' },
  { id: 'step-read', title: 'read src/theme.rs', status: 'done', at: '16:21' },
  { id: 'step-test', title: 'bash cargo test -p work-ui', status: 'done', at: '16:22' },
  { id: 'step-answer', title: '输出回答', status: 'done', at: '16:22' },
]

export const node = {
  id: 'fixture-node',
  mode: '本地节点',
  model: 'deepseek/deepseek-v4-pro',
  thinking: '高',
}

export const initialSample: WorkspaceSample = { projects, sessions, history, blocks, toolMeta, statsPayload, contextPayload, runSteps, node }
