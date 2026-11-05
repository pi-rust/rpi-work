// Source contracts, not a substitute for browser/native lifecycle acceptance.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const ui = fs.readFileSync('apps/work-ui/src/app/workspace.rs', 'utf8');
const backend = fs.readFileSync('apps/work-ui/src/features/conversation/execution_backend.rs', 'utf8');
const css = fs.readFileSync('apps/work-ui/src/features/conversation/execution_backend.css', 'utf8');
test('execution backend entry lives in the session header, never the composer', () => {
  const header = ui.slice(ui.indexOf('header { class: if workspace_page'), ui.indexOf('if *show_project_form.read()'));
  assert.match(header, /if workspace_page \{\s*execution_backend::BackendSwitcher/);
  assert.match(header, /key: "header-backend-\{node_connection.epoch\}-\{session_generation\}-\{selected_session:\?\}"/);
  assert.equal((ui.match(/execution_backend::BackendSwitcher \{/g)||[]).length, 1);
  assert.doesNotMatch(ui.slice(ui.indexOf('div { class: "composer-capabilities"')), /execution_backend::BackendMenu \{|execution_backend::BackendSwitcher \{/);
});
test('header and resource selection share guarded same-project creation, not binding mutation', () => {
  assert.match(ui, /onchange: change_backend/);
  assert.match(ui, /onbackendchange: change_backend/);
  const change = ui.slice(ui.indexOf('let change_backend ='), ui.indexOf('let requested = focus_new_session.read()'));
  for (const guard of ['new_session_busy.peek()', 'Phase::Online', 'option.available', 'session_binding.peek().is_none()', 'project.is_none()']) assert.ok(change.includes(guard));
  assert.match(change, /selected_backend.set\(backend\)/);
  assert.match(change, /new_session\(project\)/);
  assert.doesNotMatch(change, /set_binding|command_data|session_agent_binding\(/);
});
test('confirmation preserves the original session and rejects unknown state', () => {
  assert.match(backend, /!online \|\| busy \|\| \(has_session && !binding_known\)/);
  assert.match(backend, /title: "切换执行后端"/);
  assert.match(backend, /使用其他执行后端创建新会话/);
  assert.match(backend, /原会话与未发送草稿保留/);
  assert.match(backend, /has_session \{ pending.set\(Some\(id.clone\(\)\)\)/);
  assert.match(backend, /trap_focus: true/);
  assert.match(css, /@media \(max-width: 380px\)/);
  assert.match(css, /min-height: 44px/);
});
