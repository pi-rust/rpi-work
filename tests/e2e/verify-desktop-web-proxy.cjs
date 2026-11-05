// Real dev-web process + isolated loopback HTTP fixture. No real nodes/models.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const runDevWeb = require('../support/run-dev-web.cjs');
const { chromium } = require('../support/browser.cjs');

(async () => {
  const assets = path.resolve(process.env.WORK_UI_BUNDLE || 'target/web-node-navigation-candidate/parity-web');
  assert(fs.existsSync(path.join(assets, 'index.html')), 'build the formal candidate first');
  const reportDir = path.resolve(process.env.WORK_ENTRY_REPORT_DIR || 'artifacts/desktop-web-entry');
  fs.mkdirSync(reportDir, { recursive: true });
  const results = [], requests = [], unexpected = [], errors = [], proxies = [];
  let browser, failure, upstreamClosed = false;
  let declaration = { status: 200, body: JSON.stringify({ version: 1, mode: 'hosted', nodeId: 'proxy-policy-fixture' }) };
  const upstream = http.createServer((req, res) => {
    requests.push({ path: req.url, host: req.headers.host, origin: req.headers.origin, cookie: req.headers.cookie });
    if (req.url === '/.well-known/rpi-work-entry') {
      res.writeHead(declaration.status, { 'content-type': 'application/json', ...declaration.headers });
      res.end(declaration.body);
    } else if (req.url === '/api/v1/test-stream') {
      res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
      res.end('data: {"fixture":true}\n\n');
    } else {
      res.writeHead(200, { 'content-type': 'application/json', 'set-cookie': 'synthetic=ok; HttpOnly; SameSite=Strict; Path=/' });
      res.end(JSON.stringify({ fixture: true, cookie: req.headers.cookie }));
    }
  });
  await new Promise(resolve => upstream.listen(0, '127.0.0.1', resolve));
  const backendUrl = `http://127.0.0.1:${upstream.address().port}/`;
  const request = (base, target, headers = {}) => new Promise((resolve, reject) => {
    const req = http.get(new URL(target, base), { headers }, res => {
      let body = '';
      res.setEncoding('utf8'); res.on('data', data => { body += data; });
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body }));
    });
    req.setTimeout(10000, () => req.destroy(new Error('owned fixture HTTP timeout')));
    req.on('error', reject);
  });
  const start = async entryMode => {
    const proxy = runDevWeb({ backendUrl, assets, entryMode });
    proxies.push(proxy);
    const timer = setTimeout(() => proxy.stop(), 15000);
    try { return await proxy.ready; } finally { clearTimeout(timer); }
  };
  const pass = text => { results.push(text); console.log('PASS', text); };
  try {
    const web = await start(''); // Empty/unset config exercises the actual hosted default.
    const origin = new URL(web).origin;
    let response = await request(web, '/.well-known/rpi-work-entry');
    assert.equal(response.status, 200);
    assert.deepEqual(JSON.parse(response.body), JSON.parse(declaration.body));
    assert.equal(response.headers['cache-control'], 'no-store');
    assert.equal(requests.at(-1).host, new URL(backendUrl).host);
    pass('default hosted mode forwards the exact owner declaration with no-store; port zero uses actual bound Host');

    response = await request(web, '/api/v1/health', { Origin: origin, Cookie: 'synthetic=client' });
    assert.equal(response.status, 200);
    assert.equal(JSON.parse(response.body).cookie, 'synthetic=client');
    assert.match(response.headers['set-cookie'][0], /synthetic=ok/);
    assert.equal(requests.at(-1).origin, new URL(backendUrl).origin);
    assert.equal((await request(web, '/api/v1/test-stream')).body, 'data: {"fixture":true}\n\n');
    pass('API Cookie, same-origin authority translation, Set-Cookie and SSE payload survive the real proxy');

    for (const headers of [{ Host: 'untrusted.invalid' }, { Host: '[' }, { Origin: 'https://untrusted.invalid' }, { Origin: origin + '/path' }]) {
      const before = requests.length;
      assert.equal((await request(web, '/.well-known/rpi-work-entry', headers)).status, 403);
      assert.equal((await request(web, '/api/v1/health', headers)).status, 403);
      assert.equal(requests.length, before, 'rejected requests never reach backend');
    }
    const beforeUnknown = requests.length;
    assert.equal((await request(web, '/.well-known/not-a-node')).status, 404);
    assert.equal(requests.length, beforeUnknown, 'only the exact declaration path is proxied');
    pass('unknown Host, cross-origin/path-bearing Origin and unrelated well-known paths cannot reach owner APIs');

    browser = await chromium.launch({ headless: true });
    const metadataFailures = [
      { name: 'missing', status: 404, expectedStatus: 502, body: '' },
      { name: 'unavailable', status: 503, expectedStatus: 503, body: '{}' },
      { name: 'redirect', status: 302, expectedStatus: 302, body: '', headers: { location: origin + '/unexpected-redirect' } },
      { name: 'html', status: 200, expectedStatus: 200, body: '<!doctype html><script>throw new Error("metadata ran")</script>' },
      { name: 'wrong-mode', status: 200, expectedStatus: 200, body: '{"version":1,"mode":"independent","nodeId":"bad"}' },
      { name: 'missing-id', status: 200, expectedStatus: 200, body: '{"version":1,"mode":"hosted"}' },
    ];
    for (const sample of metadataFailures) {
      declaration = sample;
      const r = await request(web, '/.well-known/rpi-work-entry');
      assert.equal(r.status, sample.expectedStatus);
      if (sample.status === 404) assert.match(r.body, /Desktop Web entry is unavailable/);
      const context = await browser.newContext({ serviceWorkers: 'block' });
      try {
        await context.route(/^https?:\/\//, route => {
          const u = new URL(route.request().url());
          if (u.origin === origin) return route.continue();
          unexpected.push(route.request().url()); return route.abort();
        });
        // A syntactically valid old address points only to our owned backend;
        // consulting it would be an observable forbidden cross-origin request.
        await context.addInitScript(({ other }) => {
          localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, selected: other, recent: [other] }));
        }, { other: backendUrl });
        const page = await context.newPage();
        page.on('pageerror', error => errors.push(error.message));
        const badRequests = [];
        page.on('request', req => {
          if (/\/api\/|\/stale-node|\/unexpected-redirect/.test(new URL(req.url()).pathname)) badRequests.push(req.url());
        });
        const before = requests.length;
        await page.goto(web);
        await page.getByRole('alert').waitFor({ timeout: 15000 });
        assert.equal(await page.getByRole('button', { name: '打开设置', exact: true }).count(), 0);
        assert.equal(await page.locator('.project-entry,.session-entry').count(), 0);
        assert.deepEqual(badRequests, [], 'no auth/workspace/redirect/stale-node request after failed discovery');
        assert(requests.slice(before).every(r => r.path === '/.well-known/rpi-work-entry'));
        await page.screenshot({ path: `${reportDir}/proxy-failure-${sample.name}.png`, animations: 'disabled' });
      } finally { await context.close(); }
      pass(`${sample.name} declaration fails closed in formal WASM; no workspace, redirect or remembered-node fallback`);
    }

    const standalone = await start('standalone');
    const beforeStandalone = requests.length;
    response = await request(standalone, '/.well-known/rpi-work-entry');
    assert.equal(response.status, 404);
    assert.equal(requests.length, beforeStandalone);
    assert.equal((await request(standalone, '/')).status, 200);
    pass('explicit standalone serves the bundle with a genuine local404 declaration, never falsely binds a desktop');
    const navigated = await browser.newContext({ serviceWorkers: 'block' });
    try {
      const api = [];
      await navigated.route(/^https?:\/\//, route => {
        const url = new URL(route.request().url());
        if (url.pathname.startsWith('/api/')) api.push(url.href);
        if (url.origin === new URL(standalone).origin) return route.continue();
        unexpected.push(url.href); return route.abort();
      });
      await navigated.addInitScript(({ other }) => {
        localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, selected: other, recent: [other] }));
      }, { other: backendUrl });
      const page = await navigated.newPage();
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(standalone + '#rpi-entry=' + encodeURIComponent(new URL(web).origin));
      await page.getByRole('alert').filter({ hasText: '目标未提供宿主 Web 入口' }).waitFor({ timeout: 15000 });
      assert.equal(await page.getByRole('button', { name: '打开设置', exact: true }).count(), 0);
      assert.deepEqual(api, [], 'a selected target without hosted metadata must not load standalone history');
    } finally { await navigated.close(); }
    pass('explicit node navigation to a 404/standalone declaration fails closed instead of connecting a stale third node');

    const invalid = runDevWeb({ backendUrl, assets, entryMode: 'guess' });
    proxies.push(invalid);
    await assert.rejects(invalid.ready, /RPI_UI_ENTRY_MODE must be hosted or standalone/);
    pass('invalid deployment policy is rejected before listening');

    await new Promise(resolve => upstream.close(resolve)); upstreamClosed = true;
    response = await request(web, '/.well-known/rpi-work-entry');
    assert.equal(response.status, 502);
    assert.equal(response.headers['cache-control'], 'no-store');
    assert.deepEqual(unexpected, []); assert.deepEqual(errors, []);
    pass('unreachable owner returns uncached502 instead of independent entry; zero unexpected network or page errors');
  } catch (error) { failure = error.stack || String(error); throw error; }
  finally {
    try { if (browser) await browser.close(); }
    finally {
      try { for (const proxy of proxies) await proxy.stop(); }
      finally {
        if (!upstreamClosed) await new Promise(resolve => upstream.close(resolve));
        fs.writeFileSync(`${reportDir}/proxy-policy-validation.json`, JSON.stringify({ results, requests, unexpected, errors, failure, proxies: proxies.map(p => p.diagnostics) }, null, 2));
      }
    }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
