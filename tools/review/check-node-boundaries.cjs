// Keep Work ownership local and independent of a native HTTP plugin process.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const privatePackages = [
  ['work-component-api', 'crates/components/work-component-api'],
  ['work-http-component', 'crates/components/work-http-component'],
  ['work-http-runtime', 'crates/transport/work-http-runtime'],
  ['work-node-api', 'crates/foundation/work-node-api'],
  ['work-node-runtime', 'crates/node/work-node-runtime'],
  ['work-node-service', 'crates/transport/work-node-service'],
  ['work-node-host', 'crates/host/work-node-host'],
];
for (const [name, relativePath] of privatePackages) {
  const manifest = fs.readFileSync(path.join(root, relativePath, 'Cargo.toml'),'utf8');
  assert(/publish\s*=\s*false/.test(manifest), `${name} must remain private to Work`);
  assert(!manifest.includes('../../vendor/'), `${name} must use local Work packages`);
  assert(!manifest.includes('rpi-plugin-sdk'), `${name} must not load a plugin as its owner`);
}
const host = fs.readFileSync(path.join(root,'crates/host/work-host/src/lib.rs'),'utf8');
assert(host.includes('from_node_owner'), 'Work must compose the existing local owner');
assert(!/Process::start|--http|NODE_EXTENSION/.test(host), 'Work host must not supervise an HTTP plugin');
assert(!fs.existsSync(path.join(root,'vendor/component-packages.json')), 'Remove obsolete node vendor snapshots');
const runtimeManifest = fs.readFileSync(path.join(root,'crates/node/work-node-runtime/Cargo.toml'),'utf8');
assert(runtimeManifest.includes('git = "https://github.com/pi-rust/rpi-package"'), 'Preserve the verified native RPC backend source');
console.log('PASS Work ownership boundaries: private local packages, shared owner, no plugin host requirement');
