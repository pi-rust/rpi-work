// Exercise checkout portability without rewriting any frozen design files.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const baseline = require('../fixtures/baselines/prototype-review-baseline.json');

function check(mode) {
  const script = `
    const fs = require('node:fs');
    const original = fs.readFileSync;
    const mode = ${JSON.stringify(mode)};
    fs.readFileSync = function(file, ...args) {
      let result = original.call(this, file, ...args);
      const name = String(file).replace(/\\\\/g, '/');
      if (name.includes('/design/clarity-workbench/') && Buffer.isBuffer(result)) {
        const lf = result.toString('latin1').replace(/\\r\\n/g, '\\n');
        if (mode === 'lf') result = Buffer.from(lf, 'latin1');
        if (mode === 'crlf') result = Buffer.from(lf.replace(/\\n/g, '\\r\\n'), 'latin1');
        if (mode === 'drift') result = Buffer.concat([result, Buffer.from('changed')]);
      }
      if (mode === 'missing-script' && name.endsWith('/.github/workflows/build.yml') && typeof result === 'string') {
        result = result.replace('./apps/work-gpui/bundle-macos.sh', './apps/work-gpui/missing-bundle.sh');
      }
      return result;
    };
    require(require('node:path').join(process.cwd(), 'tools/review/check-structure.cjs'));
  `;
  return spawnSync(process.execPath, ['-e', script], { cwd: root, encoding: 'utf8' });
}

for (const mode of ['lf', 'crlf']) {
  test(`frozen design accepts ${mode.toUpperCase()} checkouts`, () => {
    const result = check(mode);
    assert.equal(result.status, 0, result.stderr);
    assert.ok(result.stdout.includes(`${Object.keys(baseline.hashes).length} unchanged design files`), result.stdout);
  });
}
for (const [mode, error] of [['drift', /Frozen design changed/], ['missing-script', /Missing workflow script/]]) {
  test(`structure guard rejects ${mode}`, () => {
    const result = check(mode);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, error);
  });
}
