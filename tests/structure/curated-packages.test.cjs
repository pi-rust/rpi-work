// Source contracts supplement formal WASM and isolated node acceptance.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const read = file => fs.readFileSync(`apps/work-ui/src/features/resources/${file}`, 'utf8');
const center = read('center.rs');
const workflow = read('workflow.rs');
const packages = read('packages.rs');

test('catalog browsing remains read-only and counts do not derive installation from acquisition', () => {
  const load = center.slice(center.indexOf('use_effect(move ||'), center.indexOf('let mut choose'));
  assert.match(load, /NodeApi::native_catalog/);
  assert.match(load, /NodeApi::package_catalog/);
  assert.doesNotMatch(load, /package_install|package_acquire|native_manage|native_apply/);
  const catalog = fs.readFileSync('crates/client/work-viewmodel/src/resources/catalog.rs', 'utf8');
  assert.match(catalog, /resource.facts\["installed"\] == true/);
  assert.doesNotMatch(catalog, /acquired/);
});

test('installation has explicit scope and authorization, canonical acquisition and authoritative readback', () => {
  assert.match(workflow, /None::<NativeScope>/);
  assert.match(workflow, /!read_only && !\*trusted.peek\(\)/);
  assert.match(workflow, /super::packages::acquired/);
  assert.match(workflow, /acquired.package != package/);
  assert.match(workflow, /NodeApi::package_install/);
  assert.match(workflow, /NodeApi::native_catalog\(client, project\)/);
  assert.match(workflow, /installed_in\(choice, &readback, scope\)/);
  assert.match(workflow, /仅回读安装结果/);
  assert.doesNotMatch(workflow, /\.acquired\s*=\s*true/);
});

test('connection owners discard pending UI state and late reads cannot overwrite another context', () => {
  assert.match(center, /connection_epoch\(\),[\s\S]*?session_generation\(\),[\s\S]*?online/);
  assert.match(center, /ResourceCenterState \{ key: "\{owner\}"/);
  for (const source of [center, workflow]) {
    assert.match(source, /connection_epoch.peek\(\) != epoch/);
    assert.match(source, /session_generation.peek\(\) != generation/);
  }
  assert.match(center, /ticket.peek\(\) != next/);
});

test('work applications preserve the original operation and configuration across retries', () => {
  assert.match(workflow, /attempt.peek\(\).clone\(\).unwrap_or_else/);
  assert.match(workflow, /target.tools = previous.tools/);
  assert.match(workflow, /target.supplementary_instructions = previous.supplementary_instructions/);
  assert.match(workflow, /NativeOperationStatus::Applied/);
  assert.match(workflow, /state.selection.as_ref\(\) != Some\(&request.target\)/);
  assert.match(workflow, /create_submitted.set\(true\)/);
});

test('downloads serialize source text into a Blob with a fixed filename', () => {
  assert.match(packages, /serde_json::to_string\(&format!\("\{text\}\\n"\)\)/);
  assert.match(packages, /new Blob/);
  assert.match(packages, /URL.revokeObjectURL/);
  assert.match(packages, /curated-resource.rpi-package.json/);
  assert.doesNotMatch(packages, /std::fs|std::process|dangerous_inner_html/);
});
