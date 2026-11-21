/**
 * Fixture content for the ReUI/Codex layout prototype.
 *
 * Same data as `tests/fixtures/ui-fixture.cjs` and the GPUI prototype, so the
 * three renderers can be compared side by side.
 */

export type Project = { id: string; name: string }

export type Session = {
  id: string
  projectId: string
  title: string
  updatedAt: number
}

export type ToolCall = {
  id: string
  name: string
  target: string
  language: string
  status: 'completed' | 'running' | 'failed'
  detail: string
  output: string
  durationMs: number
}

export type Turn = { id: string; role: 'user' | 'assistant'; text: string }

export type RunStep = { id: string; title: string; status: 'done' | 'running' | 'pending'; at: string }

export type Metrics = {
  input: number
  output: number
  cachePercent: number
  cost: number
  contextPercent: number
  contextUsed: number
  contextWindow: number
  tools: number
  failures: number
}

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

export const turns: Turn[] = [
  { id: 'user-1', role: 'user', text: '帮我为工作台增加一个深色主题，并保持手机端可用。' },
  { id: 'answer-1', role: 'assistant', text: '主题切换已经准备好，接下来运行组件测试。深色模式复用同一套 token，手机端沿用 320px 断点。' },
]

export const tools: ToolCall[] = [
  {
    id: 'read-1',
    name: 'read',
    target: 'src/theme.rs',
    language: 'rust',
    status: 'completed',
    detail: '{ "path": "src/theme.rs" }',
    output:
      'pub fn theme() -> Tokens {\n    let light = Tokens::codex(false);\n    let dark = Tokens::codex(true);\n    Tokens { light, dark }\n}',
    durationMs: 12,
  },
  {
    id: 'bash-1',
    name: 'bash',
    target: 'cargo test -p work-ui --lib',
    language: 'shell',
    status: 'completed',
    detail: '{ "command": "cargo test -p work-ui --lib" }',
    output: 'running 12 tests\ntest result: ok. 12 passed; 0 failed',
    durationMs: 1840,
  },
]

export const runSteps: RunStep[] = [
  { id: 'step-plan', title: '读取主题契约', status: 'done', at: '16:21' },
  { id: 'step-read', title: 'read src/theme.rs', status: 'done', at: '16:21' },
  { id: 'step-test', title: 'bash cargo test -p work-ui', status: 'done', at: '16:22' },
  { id: 'step-answer', title: '输出回答', status: 'done', at: '16:22' },
]

export const metrics: Metrics = {
  input: 32000,
  output: 4800,
  cachePercent: 75,
  cost: 0.0184,
  contextPercent: 38,
  contextUsed: 38000,
  contextWindow: 100000,
  tools: 2,
  failures: 0,
}

export const node = { id: 'fixture-node', mode: '本地节点', model: 'deepseek/deepseek-v4-pro', thinking: '高' }

export function formatTokens(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(1).replace('.0', '')}k` : `${value}`
}
