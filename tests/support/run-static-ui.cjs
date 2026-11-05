// Run formal UI tests against one prebuilt bundle, isolated from dev hot reload.
// Usage: node tests/support/run-static-ui.cjs tests/e2e/verify-workspace-sync.cjs [...]
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn } = require('node:child_process');

const root = process.env.WORK_UI_BUNDLE
  ? path.resolve(process.env.WORK_UI_BUNDLE)
  : path.resolve(__dirname, '../../target/parity-web');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.png': 'image/png', '.ico': 'image/x-icon' };
const server = http.createServer(async (req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, 'http://127.0.0.1').pathname);
    const file = path.resolve(root, '.' + (pathname === '/' ? '/index.html' : pathname));
    if (!file.startsWith(root + path.sep)) { res.writeHead(403); res.end(); return; }
    const body = await fs.readFile(file);
    res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
});

(async () => {
  const tests = process.argv.slice(2);
  if (!tests.length) throw new Error('Provide at least one test script');
  await fs.access(path.join(root, 'index.html'));
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const origin = `http://127.0.0.1:${server.address().port}/`;
  console.log(`Static UI (no proxy / reload): ${origin}`);
  for (const test of tests) {
    const code = await new Promise((resolve, reject) => {
      const child = spawn(process.execPath, [path.resolve(test)], { stdio: 'inherit', env: { ...process.env, WORK_UI_URL: origin } });
      child.once('error', reject);
      child.once('exit', (code, signal) => resolve(signal ? 1 : code));
    });
    if (code !== 0) { process.exitCode = code || 1; break; }
  }
})().catch(error => { console.error(error); process.exitCode = 1; })
  .finally(() => { server.close(); server.closeAllConnections(); });
