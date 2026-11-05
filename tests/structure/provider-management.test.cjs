// Source contracts only: not browser, Android or native acceptance evidence.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const source = read('apps/work-ui/src/features/settings/providers.rs');
const settings = read('apps/work-ui/src/features/settings/mod.rs');
const css = read('apps/work-ui/src/features/settings/providers.css');
const between = (a,b) => source.slice(source.indexOf(a),source.indexOf(b,source.indexOf(a)));

test('typed node-wide catalog reads on open, never creates hosts or probes URLs', () => {
  assert.match(source, /use_effect\(move \|\|\s*\{\s*refresh.call\(\(\)\)/);
  assert.match(source, /NodeApi::provider_catalog/);
  assert.match(source, /NodeApi::provider_mutate/);
  assert.match(source, /NodeApi::provider_release_hosts/);
  assert.doesNotMatch(source, /control_session\(|command_data|set_model\(|set_api_key|reqwest|localStorage|std::fs|std::process|dangerous_inner_html/);
  assert.doesNotMatch(settings, /base_url_draft|provider_session|AuthStatus|ModelDescriptor|section == "providers" && !mobile_client/);
  assert.match(source, /返回会话选择模型/);
});
test('entire state owner is keyed and live freshness guards every awaited result', () => {
  const key = between('let key =','fn ProviderSettingsState');
  for (const field of ['epoch','generation','session','online']) assert.ok(key.includes(`context.${field}`));
  assert.match(key, /ProviderSettingsState \{ key:/);
  assert.match(source, /expected == current && current.online && ticket == latest/);
  assert.match(source, /online: \*live_online.peek\(\)/);
  assert.equal((source.match(/\.await;\s*if !is_fresh\(&expected, next\)/g)||[]).length,4);
  const workspace = read('apps/work-ui/src/app/workspace.rs');
  assert.match(workspace,/SettingsPage \{[\s\S]*?connection_epoch: node_connection.epoch, online: matches!\(\*node_connection.phase.read\(\), connections::Phase::Online\)/);
});
test('CAS, intended readback, pending lock and explicit recovery instead of retry', () => {
  assert.match(source, /expected_revision: observed.revision/);
  assert.match(source, /receipt_revision == catalog.revision && change.matches\(catalog\)/);
  assert.match(source, /online && observed && !busy && !pending/);
  assert.match(source, /pending.set\(Some\(change.clone\(\)\)\)/);
  assert.match(source, /bounded\(NodeApi::provider_mutate[\s\S]*?bounded\(NodeApi::provider_catalog/);
  assert.match(source, /15_000/);
  assert.match(source, /不会自动重试/);
  assert.match(source, /高级定义只读/);
  assert.match(source, /entry.configured && entry.definition.is_none\(\)/);
});
test('definition saves never submit secrets; write-only secret is consumed and error bodies hidden', () => {
  const save = between('let save_definition =','let submit_key =');
  assert.match(save,/ProviderAction::Upsert/);
  assert.doesNotMatch(save,/SetKey/);
  const key = between('let submit_key =','let confirm_action =');
  assert.match(key,/std::mem::take\(&mut \*secret.write\(\)\)/);
  assert.match(key,/ProviderAction::SetKey \{ id, key \}/);
  assert.match(source,/r#type: "password", autocomplete: "new-password"/);
  assert.match(source,/use_drop\(move \|\|/);
  assert.doesNotMatch(source,/format!\([^\n]*error|println!|tracing::|console::/);
  assert.doesNotMatch(between('enum ExpectedChange','impl ExpectedChange'),/key: String/);
});
test('destructive actions use explicit Modal confirmations and source-specific observations', () => {
  for (const action of ['Remove','ClearStored','ClearConfig','ReplaceKey']) assert.ok(source.includes(`Confirmation::${action}`));
  assert.match(source,/provider-confirmation/);
  assert.match(source,/CredentialLocation::Stored/);
  assert.match(source,/CredentialLocation::Config/);
  assert.match(source,/清除已存 OAuth/);
  assert.match(source,/不撤销远端授权/);
  assert.match(source,/已保存的凭据会保留并继续显示/);
  assert.match(source,/report.released.len\(\), report.blocked.len\(\)/);
  assert.match(source,/仅保证单文件原子替换/);
  assert.match(source,/不会连接测试新 URL/);
});
test('shared responsive form has theme tokens, labels, wrapping and touch affordances', () => {
  for (const token of ['var(--text)','var(--space-','min-height:44px','overflow-wrap:anywhere','focus-visible','font-size:max(16px,var(--font-body))']) assert.ok(css.includes(token));
  assert.doesNotMatch(css,/#[0-9a-f]{3,8}\b|rgb\(/i);
  assert.doesNotMatch(source,/target_os = "android"|mobile_client|is_local\(/);
  assert.match(source,/provider-model-context-\{index\}/);
  assert.match(source,/Edit::AddModel/);
  assert.match(source,/Edit::RemoveModel/);
});
