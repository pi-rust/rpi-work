// Formal desktop Web/WASM only. Real random-port HTTP fixtures; QR issuance is
// intentionally simulated. No Android, real-server protocol or hosted-gate claim.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const nodes = [], results = [], issues = [], unexpectedRequests = [];
  let browser, page, pageErrorCount = 0, failed = false, stage = 'setup';
  let held = null, releaseHeld = null, ttl = 120;
  const check = (condition, label) => assert.ok(condition, label);
  const pass = label => { results.push(label); console.log('PASS', label); };
  const until = async (predicate, label, timeout = 10000) => {
    const end = Date.now() + timeout;
    while (Date.now() < end) {
      if (await predicate()) return;
      await new Promise(resolve => setTimeout(resolve, 40));
    }
    throw new Error(label);
  };
  try {
    for (const [address, id, char] of [
      ['http://192.168.50.22:3031', 'qr-first', 'a'],
      ['http://192.168.50.23:3031', 'qr-second', 'b'],
    ]) nodes.push({ fixture: await startFixture(), address, id, char });
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, serviceWorkers: 'block' });
    await context.addInitScript(() => {
      window.__qrCopyCount = 0;
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: {
        writeText: async () => { window.__qrCopyCount++; },
      } });
    });
    page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', () => { pageErrorCount++; });
    const ui = new URL(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    // Never let browser requests reach real LAN/external services. Alias routing
    // below is the same route.fetch -> fixture URL pattern as connection-share.
    await page.route('**/*', route => {
      const url = new URL(route.request().url());
      if (url.origin === ui.origin) return route.continue();
      unexpectedRequests.push(1);
      return route.abort();
    });
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    for (const node of nodes) {
      await page.route(`${node.address}/**`, async route => {
        const request = route.request(), url = new URL(request.url());
        if (request.method() === 'POST' && url.pathname === '/api/v1/auth/qr') {
          const grant = { ticket: node.char.repeat(64), expiresAt: Math.floor(Date.now() / 1000) + ttl, nodeId: node.id };
          const issue = { node, grant, fulfilled: false };
          issues.push(issue);
          if (held === node) {
            held = null;
            await new Promise(resolve => {
              const timer = setTimeout(resolve, 8000);
              releaseHeld = () => { clearTimeout(timer); resolve(); };
            });
          }
          await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(grant) });
          issue.fulfilled = true;
          return;
        }
        const response = await route.fetch({ url: new URL(url.pathname + url.search, node.fixture.url).href });
        if (request.method() === 'GET' && url.pathname === '/api/v1/health') {
          await route.fulfill({ response, json: { ...await response.json(), nodeId: node.id, sharingEnabled: true } });
        } else await route.fulfill({ response });
      });
    }
    const trigger = () => page.getByRole('button', { name: '手机 App 扫码授权', exact: true });
    const dialog = () => page.getByRole('dialog', { name: '手机 App 扫码授权', exact: true });
    const svg = () => dialog().locator('#device-auth-qr-code svg');
    const online = () => page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    const decode = async () => {
      const raw = await svg().evaluate(async element => {
        const clone = element.cloneNode(true);
        clone.setAttribute('width', '768'); clone.setAttribute('height', '768');
        const url = URL.createObjectURL(new Blob([clone.outerHTML], { type: 'image/svg+xml' }));
        try {
          const image = new Image(); image.src = url;
          await Promise.race([image.decode(), new Promise((_, reject) => setTimeout(() => reject(new Error('QR raster timeout')), 3000))]);
          const canvas = document.createElement('canvas'); canvas.width = canvas.height = 768;
          const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0, 768, 768);
          return window.jsQR(ctx.getImageData(0, 0, 768, 768).data, 768, 768)?.data || '';
        } finally { URL.revokeObjectURL(url); }
      });
      // Never pass decoded material to assertion diff formatting or diagnostics.
      try { return JSON.parse(raw); } catch { throw new Error('QR did not decode as JSON'); }
    };
    const verify = async issue => {
      const value = await decode();
      const expected = { v: 1, type: 'rpi-work-node-auth', address: issue.node.address, ...issue.grant };
      check(Object.keys(value).sort().join(',') === Object.keys(expected).sort().join(','), 'QR has exactly the protocol fields');
      check(Object.entries(expected).every(([key, val]) => value[key] === val), 'QR fields match the current node and issued grant');
    };
    const privateUi = async () => {
      const safe = await page.evaluate(tickets => {
        const modal = document.querySelector('#device-auth-qr');
        const storage = JSON.stringify(Object.entries(localStorage));
        const body = document.body.innerText;
        return !tickets.some(ticket => body.includes(ticket) || storage.includes(ticket))
          && !storage.includes('rpi-work-node-auth')
          && !modal?.querySelector('textarea,input[type=password]')
          && !Array.from(modal?.querySelectorAll('input,textarea,button') || []).some(el =>
            /复制|copy/i.test(el.textContent) || tickets.some(ticket => (el.value || '').includes(ticket)))
          && !/test-only|fixture-session-only|rpi_web_session|"ticket"|"password"|"cookie"/i.test(modal?.innerText || '')
          && window.__qrCopyCount === 0;
      }, nodes.map(node => node.char.repeat(64)));
      check(safe, 'No raw ticket, copyable JSON, password, cookie or persisted authorization');
    };
    const open = async () => {
      check(await trigger().isEnabled(), 'Authorization button enabled after login');
      await trigger().click(); await svg().waitFor();
    };
    stage = 'login and initial QR';
    await page.goto(ui.href);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.addScriptTag({ content: fs.readFileSync('assets/vendor/jsQR.js', 'utf8') });
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    const address = page.getByLabel('节点地址', { exact: true });
    const connect = page.getByRole('button', { name: '连接节点', exact: true });
    await address.fill(nodes[0].address);
    await page.getByLabel('节点密码', { exact: true }).fill('test-only');
    await connect.click(); await online();
    check(nodes[0].fixture.requests.some(r => r.method === 'POST' && r.path === '/api/v1/auth' && r.body.password === 'test-only'), 'Fixture login used test-only password');
    await open(); await verify(issues.at(-1)); await privateUi();
    check(issues.length === 1 && issues[0].node === nodes[0], 'Only current client issued first QR');
    await page.locator('.device-auth-entry > button').evaluate(button => button.click());
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    check(issues.length === 1 && await page.locator('#device-auth-qr').count() === 1, 'Repeated open reuses the valid global QR without issuing again');
    pass('Desktop Web login enables authorization; generated SVG decodes to exact v1 grant fields');

    stage = 'draft invalidation';
    // Dispatch input explicitly: the modal focus trap can redirect Playwright's
    // fill before its native input event reaches the background form.
    await address.evaluate((input, value) => {
      input.value = value;
      input.dispatchEvent(new Event('input', { bubbles: true }));
    }, nodes[1].address);
    stage = 'draft closes dialog';
    await dialog().waitFor({ state: 'hidden' });
    check(await page.locator('#device-auth-qr-code svg').count() === 0, 'Draft edit removes QR pixels');
    stage = 'draft reopen';
    await open();
    stage = 'draft QR exact fields';
    await verify(issues.at(-1));
    stage = 'draft privacy';
    await privateUi();
    check(issues.length === 2 && issues.every(issue => issue.node === nodes[0]), 'Reopen posts only to current node, never draft');
    pass('Editing base_url closes authorization; reopen uses current node rather than draft');
    await dialog().getByRole('button', { name: '关闭授权码', exact: true }).click();

    stage = 'late issuance across node switch';
    held = nodes[0];
    await trigger().click();
    await until(() => issues.length === 3 && releaseHeld !== null, 'Delayed issuance was not reached');
    const late = issues.at(-1);
    // The modal intentionally blocks normal interaction; invoke the existing
    // connection action behind it to exercise connection-epoch invalidation.
    await page.evaluate(() => document.querySelector('.remote-node-form button').click());
    await dialog().waitFor({ state: 'hidden' }); await online();
    check((await page.locator('.node-address').innerText()).includes('192.168.50.23'), 'Second node is current');
    await open();
    check(issues.length === 4 && issues.at(-1).node === nodes[1], 'New current client issues QR');
    await verify(issues.at(-1));
    releaseHeld(); releaseHeld = null;
    await until(() => late.fulfilled, 'Delayed response did not finish');
    // Bounded settling window for the old WASM future; no unbounded sleep/poll.
    await page.waitForTimeout(300);
    await verify(issues.at(-1)); await privateUi();
    pass('Node switch uses the new client and late old issuance cannot replace its QR');

    stage = 'expiry removes pixels';
    await dialog().getByRole('button', { name: '关闭授权码', exact: true }).click();
    ttl = 3;
    await open(); await verify(issues.at(-1)); await privateUi();
    check(issues.length === 5 && issues.at(-1).node === nodes[1], 'Short grant issued only by second node');
    await dialog().getByRole('alert').filter({ hasText: '授权二维码已过期，已隐藏；请重新生成。' }).waitFor({ timeout: 6000 });
    check(await page.locator('#device-auth-qr-code svg, #device-auth-qr-code canvas, #device-auth-qr-code img').count() === 0, 'No lingering expired QR pixels');
    await privateUi();
    await page.waitForTimeout(1200);
    check(issues.length === 5, 'Expiry never mints a replacement authorization');
    pass('Three-second grant first renders, then expires with explanatory text and no lingering SVG or automatic reissue');

    stage = 'global lifetime across Settings remount';
    // Force page navigation behind the modal to exercise disposal/invalidation,
    // not normal pointer behavior (which the dialog intentionally traps).
    await page.locator('.settings-navigation nav button').filter({ hasText: /^通用$/ }).evaluate(button => button.click());
    await dialog().waitFor({ state: 'hidden' });
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    check(issues.length === 5, 'Reentering Devices does not automatically issue');
    await open(); await verify(issues.at(-1)); await privateUi();
    check(issues.length === 6 && await page.locator('#device-auth-qr').count() === 1, 'Settings remount reuses one workspace authorization instance');
    await dialog().getByRole('button', { name: '关闭授权码', exact: true }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    check(issues.length === 6 && await page.locator('#device-auth-qr').count() === 0, 'Layout changes do not reopen or reissue');
    pass('Workspace-global authorization is single-instance across Settings and layout changes; opening a valid QR twice never reissues it');
    check(pageErrorCount === 0 && unexpectedRequests.length === 0, 'No browser errors or unexpected network requests');
    pass('No ticket text/copy/storage leakage and no real LAN/external requests');
  } catch {
    failed = true;
    // Do not print original errors: assertion/browser diagnostics may contain
    // response bodies or SVG payloads. No screenshots, traces or DOM artifacts.
    console.error(`FAIL authorization QR: ${stage}`);
    process.exitCode = 1;
  } finally {
    releaseHeld?.();
    await browser?.close().catch(() => {});
    for (const node of nodes) await node.fixture.close();
    fs.writeFileSync('artifacts/authorization-qr-validation.json', JSON.stringify({
      scope: 'Formal desktop Web WASM; isolated HTTP fixtures; fake QR issuance; actual SVG jsQR decode; not real Android or real-server protocol validation',
      results, passed: results.length, failed: failed ? 1 : 0,
      issuanceCount: issues.length, issuanceCountsByNode: nodes.map(node => issues.filter(issue => issue.node === node).length),
      pageErrorCount, unexpectedRequestCount: unexpectedRequests.length,
    }, null, 2));
  }
})();
