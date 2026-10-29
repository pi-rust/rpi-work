// No provider calls: verify the proxy's origin boundary against an isolated node.
const http = require('node:http');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const once = require('node:events').once;

(async () => {
  let count = 0, child;
  const upstream = http.createServer((request, response) => {
    count++;
    const host = `127.0.0.1:${upstream.address().port}`;
    if (request.headers.host !== host || (request.headers.origin && ![`http://${host}`, `https://${host}`].includes(request.headers.origin))) {
      response.writeHead(403); response.end(); return;
    }
    response.writeHead(200, { 'content-type': 'application/json', 'set-cookie': 'test-only=value; HttpOnly; SameSite=Strict; Path=/' });
    response.end(JSON.stringify({ path: request.url, method: request.method, origin: request.headers.origin, marker: request.headers['x-rpi-web'] }));
  });
  await new Promise(resolve => upstream.listen(0, '127.0.0.1', resolve));
  const reservation = http.createServer();
  await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  const origin = `http://127.0.0.1:${port}`;
  const request = (headers = {}, method = 'GET') => new Promise((resolve, reject) => {
    const call = http.request(origin + '/api/v1/auth', { method, headers }, response => {
      let body = ''; response.on('data', data => body += data); response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, body: body && JSON.parse(body) }));
    });
    call.on('error', reject); call.end();
  });
  const results = [];
  const pass = name => { results.push(name); console.log('PASS', name); };
  try {
    child = spawn(process.execPath, ['tools/dev/dev-web.mjs'], { env: { ...process.env, PORT: String(port), RPI_UI_HOST: '127.0.0.1', RPI_API_HOST: '127.0.0.1', RPI_API_PORT: String(upstream.address().port), RPI_UI_ALLOWED_HOSTS: '' }, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    await Promise.race([
      new Promise((resolve, reject) => {
        let text = ''; child.stdout.on('data', data => { text += data; if (text.includes('local URL:')) resolve(); });
        child.once('error', reject); child.once('exit', code => reject(new Error(`Proxy exited: ${code}`)));
      }),
      new Promise((_, reject) => { const timer = setTimeout(() => reject(new Error('Proxy startup timeout')), 10000); timer.unref(); }),
    ]);
    const health = await request(); assert.equal(health.status, 200);
    pass('a local API client without Origin can reach the configured node');
    const login = await request({ origin, 'x-rpi-web': '1' }, 'POST');
    assert.equal(login.status, 200); assert.equal(login.body.origin, `http://127.0.0.1:${upstream.address().port}`); assert.equal(login.body.marker, '1');
    assert(login.headers['set-cookie'][0].includes('HttpOnly'));
    pass('a same-origin mutation translates Origin and preserves the request marker and authentication cookie attributes');
    const secure = await request({ origin: origin.replace('http:', 'https:'), 'x-rpi-web': '1' }, 'POST');
    assert.equal(secure.status, 200); assert.match(secure.body.origin, /^https:/);
    pass('an HTTPS public origin keeps its secure scheme when forwarding to the node');
    const before = count;
    for (const headers of [
      { origin: 'https://foreign.example' },
      { origin: 'null' },
      { origin: origin + '/path' },
      { host: 'rebound.example', origin: 'http://rebound.example' },
      { host: 'rebound.example' },
    ]) assert.equal((await request(headers, 'POST')).status, 403);
    assert.equal(count, before);
    pass('foreign/null/path origins and unapproved Host authorities are rejected before reaching the node');
    fs.writeFileSync('artifacts/dev-proxy-validation.json', JSON.stringify({ passed: true, scope: 'isolated strict origin-checking HTTP node, no credentials or provider calls', results }, null, 2));
  } finally {
    if (child && child.exitCode === null) { const stopped = once(child, 'exit'); child.kill(); await stopped; }
    upstream.closeAllConnections(); await new Promise(resolve => upstream.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
