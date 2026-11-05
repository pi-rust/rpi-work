'use strict';
// Real fixed-default listener smoke. Fails on occupied ports; never attaches to
// or stops an existing server. Only an owned in-memory API fixture is queried.
const assert = require('node:assert/strict');
const http = require('node:http');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const root = path.resolve(__dirname, '../..');
const children = [];
const api = http.createServer((req, res) => {
  res.writeHead(200, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ fixture: 'default-port-proxy', path: req.url }));
});
async function start(script, banner) {
  const env = { ...process.env, RPI_UI_HOST: '127.0.0.1', RPI_API_HOST: '127.0.0.1' };
  delete env.PORT;
  delete env.RPI_API_PORT;
  const child = spawn(process.execPath, [script], { cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  children.push(child);
  await new Promise((resolve, reject) => {
    let output = '';
    const timer = setTimeout(() => reject(new Error('Owned preview listener startup timed out')), 10000);
    const fail = () => { clearTimeout(timer); reject(new Error('Owned preview failed; no existing service was queried')); };
    child.once('error', fail);
    child.once('exit', fail);
    child.stderr.on('data', () => {});
    child.stdout.on('data', data => {
      output += data.toString();
      if (output.includes(banner)) {
        clearTimeout(timer);
        child.removeListener('error', fail);
        child.removeListener('exit', fail);
        resolve();
      }
    });
  });
}
async function stop(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const exited = once(child, 'exit');
  child.kill();
  const timer = setTimeout(() => { if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL'); }, 3000);
  try { await exited; } finally { clearTimeout(timer); }
}
(async () => {
  try {
    await new Promise((resolve, reject) => {
      api.once('error', reject);
      api.listen(13030, '127.0.0.1', resolve);
    });
    await start('tools/dev/dev-web.mjs', 'local URL: http://127.0.0.1:14180/');
    await start('tools/review/ui-review-server.mjs', 'Visual review: http://127.0.0.1:14182/prototype/');
    const request = url => fetch(url, { signal: AbortSignal.timeout(5000) });
    const ui = await request('http://127.0.0.1:14180/');
    assert.equal(ui.status, 200);
    assert.match(ui.headers.get('content-type'), /text\/html/);
    const response = await request('http://127.0.0.1:14180/api/health');
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { fixture: 'default-port-proxy', path: '/api/health' });
    assert.equal((await request('http://127.0.0.1:14182/prototype/')).status, 200);
    console.log('PASS real unset-port Web 14180 proxies only to the owned default API fixture 13030');
    console.log('PASS real unset-port review listener serves the reference on 14182');
  } finally {
    for (const child of children.reverse()) await stop(child);
    if (api.listening) {
      api.closeAllConnections();
      await new Promise(resolve => api.close(resolve));
    }
  }
})().catch(() => { console.error('FAIL fixed-default listener smoke (check port availability/build assets); all owned children cleaned up'); process.exitCode = 1; });
