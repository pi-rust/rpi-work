// Source-contract checks, not browser geometry or native-install evidence.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const root = path.resolve(__dirname, '../..');
const read = (file) => fs.readFileSync(path.join(root, file), 'utf8');
const source = read('apps/work-ui/src/features/resources/packages.rs');
const css = read('apps/work-ui/src/features/resources/packages.css');

const between = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));

test('catalog and preview are typed reads, never acquisition or installation', () => {
  const load = between('use_effect(move ||', 'let mut act =');
  assert.match(load, /NodeApi::package_catalog/);
  assert.doesNotMatch(load, /package_acquire|package_install/);
  const preview = between('Action::Preview | Action::Download =>', 'Action::Acquire =>');
  assert.match(preview, /NodeApi::package_bundle/);
  assert.doesNotMatch(preview, /package_acquire|package_install/);
  assert.match(source, /非在线市场/);
  assert.match(source, /不含二进制/);
  assert.doesNotMatch(source, /dangerous_inner_html|std::fs|std::process|native_apply|native_manage/);
});

test('entire state owner remounts for context and every async result checks freshness', () => {
  const key = between('let key =', '#[component]\nfn PackageDiscoveryState');
  for (const field of ['project', 'epoch', 'generation', 'online']) {
    assert.ok(key.includes(`context.${field}`));
  }
  assert.match(key, /for owner in \[key\]/);
  assert.match(key, /PackageDiscoveryState \{ key:/);
  assert.match(source, /expected == current && current.online && ticket == latest/);
  // Five awaits inside the owner: catalog, bundle, download, acquire, install.
  const owner = between('fn PackageDiscoveryState(', 'async fn download_bundle');
  assert.equal((owner.match(/\.await;/g) || []).length, 5);
  assert.equal((owner.match(/\.await;\s*if !is_fresh\(&expected, next\)/g) || []).length, 5);
  const admission = between('let mut act =', 'spawn(async move {\n            match action');
  assert.match(admission, /if !is_fresh\(&expected, \*ticket.peek\(\)\)/);
});

test('install requires exact acquired catalog entry and explicit scope/trust; only successful receipt refreshes native controls', () => {
  assert.match(source, /reference\(p\) == \*target && p.acquired/);
  assert.match(source, /online && !busy && acquired && trusted/);
  assert.match(source, /acquired\(catalog.peek\(\).as_ref\(\), &package\)/);
  assert.match(source, /trusted: confirmed_trust/);
  assert.match(source, /Ok\(receipt\) if receipt.success => \{[\s\S]*?onchanged.call\(\(\)\);/);
  assert.match(source, /Ok\(receipt\) => error.set/);
  assert.match(source, /scope.set\([\s\S]*?trusted.set\(false\)/);
  assert.match(source, /target.set\(Some\(item_ref.clone\(\)\)\);[\s\S]*?trusted.set\(false\)/);
  assert.match(source, /let mut trusted = use_signal\(\|\| false\)/);
  assert.doesNotMatch(source, /\.acquired\s*=\s*true/);
  const center = read('apps/work-ui/src/features/resources/center.rs');
  assert.match(center, /if discovered\(\)/);
  assert.match(center, /super::packages::PackageDiscovery/);
  assert.match(center, /onchanged: move \|_\| \{ revision \+= 1; \}/);
  assert.match(center, /NativeResources \{[\s\S]*?revision: revision\(\)/);
  assert.match(read('apps/work-ui/src/features/resources/native.rs'), /&online, &revision/);
});

test('download uses escaped UTF-8 JSON Blob and reports a request, not filesystem success', () => {
  assert.match(source, /serde_json::to_string\(&format!\("\{text\}\\n"\)\)/);
  assert.match(source, /new Blob/);
  assert.match(source, /application\/json;charset=utf-8/);
  assert.match(source, /URL.revokeObjectURL/);
  assert.match(source, /document::eval/);
  assert.match(source, /已请求下载/);
});

test('responsive shared presentation has theme tokens, wrapping and keyboard/touch affordances', () => {
  assert.match(css, /var\(--text\)/);
  assert.match(css, /var\(--space-/);
  assert.match(css, /minmax\(min\(100%,16rem\),1fr\)/);
  assert.match(css, /overflow-wrap:anywhere/);
  assert.match(css, /min-height:44px/);
  assert.match(css, /focus-visible/);
  assert.match(css, /font-size:max\(16px,var\(--font-body\)\)/);
  assert.doesNotMatch(css, /#[0-9a-f]{3,8}\b|rgb\(/i);
});
