// Architecture guardrails complement the executable shared-screen browser harness.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const read = name => fs.readFileSync(name, 'utf8');

test('all connection controllers mount the complete production screen', () => {
  for (const file of ['app/entry.rs','app/mobile_entry.rs','features/settings/devices.rs']) {
    const source=read(`apps/work-ui/src/${file}`);
    assert.match(source,/ConnectionScreen\s*[{]/);
    assert.doesNotMatch(source,/(ConnectionTabs|PasswordConnectionForm|ConnectionScanner)\s*[{]/);
  }
  const screen=read('apps/work-ui/src/features/settings/connection_panel.rs').split('#[cfg(test)]')[0];
  assert.doesNotMatch(screen,/target_os|target_arch|cfg!/);
});

test('workspace navigation and connection actions do not fork by platform', () => {
  const workspace=read('apps/work-ui/src/app/workspace.rs');
  const defaults=workspace.slice(workspace.indexOf('let mut active_page ='),workspace.indexOf('let mut session_details_open ='));
  assert.doesNotMatch(defaults,/target_os|target_arch|cfg!/);
  assert.match(defaults,/active_page = use_signal\(\|\| "workspace"/);
  assert.match(defaults,/settings_section = use_signal\(\|\| "settings"/);
  const devices=read('apps/work-ui/src/features/settings/devices.rs');
  const presentation=devices.slice(devices.indexOf('ConnectionScreen {'),devices.indexOf('if !connection.warning'));
  assert.doesNotMatch(presentation,/target_os|target_arch|cfg!/);
  assert.match(presentation,/session_control.and_then\(\|control\| control.restart\)/);
  assert.match(presentation,/restart.call/);
  assert.match(presentation,/control.forget/);
  assert.doesNotMatch(devices,/MobileEntryControl|native_mobile/);
  assert(devices.includes('if owned_local || (!mobile_client && !scoped_entry)')); 
  for (const file of ['entry.rs','mobile_entry.rs']) {
    assert.match(read(`apps/work-ui/src/app/${file}`),/use_context_provider\(move \|\| ConnectionSessionControl/);
  }
});

test('hosted login stays fixed while the workspace offers capability-driven node navigation', () => {
  const entry=read('apps/work-ui/src/app/entry.rs');
  assert.match(entry,/address, fixed_address: true, fixed_target: true/);
  assert.match(entry,/restart: None/);
  assert.match(entry,/node_navigation::use_navigation/);
  assert.doesNotMatch(entry.slice(entry.indexOf('pub(crate) struct ConnectionSessionControl'),entry.indexOf('pub(crate) struct EntryNode')),/fixed_target/);
  const workspace=read('apps/work-ui/src/app/workspace.rs');
  const connect=workspace.slice(workspace.indexOf('let mut connect_node ='),workspace.indexOf('let candidate ='));
  assert.match(connect,/navigation.navigate.call\(target\)/);
  assert.match(connect,/NodePassword::default/);
  const screen=read('apps/work-ui/src/features/settings/connection_panel.rs');
  assert.match(screen,/返回入口节点/);
  assert.match(screen,/if show_password/);
});

test('single UI rule is an explicit development constraint, including security and evidence', () => {
  const rules=read('docs/guides/development-standards.md');
  for (const text of ['跨平台单一 UI','平台分支止于能力适配层','安全策略不是平台皮肤','共享源码不等于视觉已验收','ConnectionScreen','ConnectionSessionControl']) assert(rules.includes(text),text);
});
