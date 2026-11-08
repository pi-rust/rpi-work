'use strict';
// Configuration contract only: no running service, credentials or model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');

const node = read('crates/node/work-node/src/lib.rs');
assert.match(node, /port: 13030,/);
assert.match(read('apps/work-cli/src/args.rs'), /const DEFAULT_URL: &str = "http:\/\/127\.0\.0\.1:13031\/"/);
assert.match(read('crates/host/work-host/src/http.rs'), /env::var\("RPI_WORK_HTTP_PORT"\)\s*\.unwrap_or_else\(\|_\| "13031"/);
assert.match(read('tools/dev/migrate-idle-sessions.py'), /add_argument\("--port", type=int, default=13030\)/);
const task = read('Taskfile.yml');
assert.match(task, /PORT: '\{\{\.PORT \| default "14180"\}\}'/);
assert.match(task, /RPI_WORK_HTTP_WEB_PORT: '\{\{\.PORT \| default "14180"\}\}'/);
assert.match(task, /RPI_API_PORT: '\{\{\.RPI_WORK_HTTP_PORT \| default "13031"\}\}'/);
assert.match(task, /API_PORT: '\{\{\.RPI_API_PORT \| default "13030"\}\}'/);
console.log('PASS CLI defaults to shared desktop/headless owner 13031; standalone and migration remain explicit 13030');

function portValue(file, variable, env) {
  const declaration = read(file).match(new RegExp(`const ${variable} = ([^;]+);`));
  assert(declaration, `${file}: ${variable} declaration missing`);
  return vm.runInNewContext(declaration[1], { process: { env } });
}
for (const [file, variable, key, expected] of [
  ['tools/dev/dev-web.mjs', 'port', 'PORT', 14180],
  ['tools/dev/dev-web.mjs', 'apiPort', 'RPI_API_PORT', 13031],
  ['tools/review/ui-review-server.mjs', 'port', 'PORT', 14182],
]) {
  assert.equal(portValue(file, variable, {}), expected);
  assert.equal(portValue(file, variable, { [key]: '' }), expected);
  assert.equal(portValue(file, variable, { [key]: '8123' }), 8123);
  assert.equal(portValue(file, variable, { [key]: '0' }), 0);
}
assert.match(task, /RPI_UI_ENTRY_MODE: hosted/);
assert.match(read('tools/dev/dev-web.mjs'), /const entryMode = process\.env\.RPI_UI_ENTRY_MODE \|\| 'hosted'/);
console.log('PASS development/review defaults exceed 10000; Web defaults to desktop hosted mode; explicit low ports and ephemeral port zero still override');
assert.match(read('tools/dev/build-ui-web.ps1'), /Review URL: http:\/\/127\.0\.0\.1:14180\//);
for (const file of ['README.md', 'docs/guides/desktop-http.md', 'docs/guides/node-connection.md']) {
  assert(read(file).includes('13031'), `${file}: desktop documentation missing`);
  assert(read(file).includes('14180'), `${file}: web documentation missing`);
}
console.log('PASS build hints and current connection guides use the migrated desktop/Web addresses');
