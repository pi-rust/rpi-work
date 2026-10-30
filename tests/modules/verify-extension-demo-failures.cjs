// Negative paths use the real runner/host, never a real external model.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
const path = require('node:path');
process.chdir(path.resolve(__dirname, '../..'));
const report = () => JSON.parse(fs.readFileSync('artifacts/extension-demo-validation.json', 'utf8'));
const invoke = (args, options = {}) => spawnSync(process.execPath, ['tools/dev/extension-demo.mjs', ...args], { encoding: 'utf8', timeout: 180000, ...options });
const results = [];

const failed = invoke(['--verify', '--fail-after-tool']);
assert.ifError(failed.error); assert.equal(failed.status, 1, failed.stderr);
let evidence = report();
assert.equal(evidence.count, 4); assert.equal(evidence.settlements[0].outcome, 'failed');
assert(evidence.failure); assert.equal(evidence.cleanupVerified, true);
assert.equal(evidence.installed, false); assert.equal(evidence.loaded, false);
results.push('completed native tool does not conceal failed Agent settlement; cleanup still unlinks');

const absent = invoke(['--verify'], { env: { ...process.env, RPI_WORK_SMOKE_BIN: path.resolve('target/no-such-demo-host') }, timeout: 10000 });
assert.ifError(absent.error); assert.equal(absent.status, 1, absent.stderr);
evidence = report(); assert(evidence.failure); assert.equal(evidence.cleanupVerified, true);
results.push('missing executable fails without hanging and writes failure evidence');

const stopped = invoke([], { input: 'stop\n' });
assert.ifError(stopped.error); assert.equal(stopped.status, 0, stopped.stderr);
evidence = report(); assert.equal(evidence.stopped, true); assert.equal(evidence.cleanupVerified, true);
assert.equal(evidence.installed, false); assert.equal(evidence.loaded, false);
results.push('stop during startup waits for the in-flight operation and cleans the isolated project');

fs.writeFileSync('artifacts/extension-demo-negative-validation.json', JSON.stringify({ results }, null, 2));
for (const result of results) console.log('PASS', result);
