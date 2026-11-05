// Real Dioxus UI, real QR video frames; camera hardware/permissions are simulated.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const node = await startFixture(0, { authRequired: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const results = [], errors = [];
  let completed = false;
  const address = 'http://192.168.50.9:3030';
  const payload = JSON.stringify({ v: 1, type: 'rpi-work-node-address', address });
  const pass = text => { results.push(text); console.log('PASS', text); };
  try {
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
    await context.route(`${address}/**`, async route => {
      const response = await route.fetch({ url: new URL(new URL(route.request().url()).pathname, node.url).href });
      await route.fulfill({ response });
    });
    await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await context.addInitScript(value => {
      window.__payload = value;
      window.__cameraMode = 'constraints';
      window.__cameraCalls = [];
      window.__stopped = 0;
      const originalTimeout = window.setTimeout.bind(window);
      window.setTimeout = (callback, delay, ...args) => originalTimeout(callback, window.__fastTimeout && delay === 30000 ? 200 : delay, ...args);
      window.__fakeStream = () => {
        const stream = new MediaStream();
        Object.defineProperty(stream, 'getTracks', { value: () => [{ stop() { window.__stopped++; } }] });
        return stream;
      };
      Object.defineProperty(navigator.mediaDevices, 'getUserMedia', { value: async constraints => {
        window.__cameraCalls.push(constraints);
        if (window.__cameraMode === 'deny') throw new DOMException('denied', 'NotAllowedError');
        if (window.__cameraMode === 'error') throw new DOMException('camera unavailable', 'NotReadableError');
        if (window.__cameraMode === 'pending') return new Promise(resolve => { window.__resolveCamera = resolve; });
        if (window.__cameraMode === 'constraints' && typeof constraints.video === 'object') throw new DOMException('rear camera unavailable', 'OverconstrainedError');
        if (window.__cameraMode === 'video') return window.__videoStream;
        return window.__fakeStream();
      } });
      window.BarcodeDetector = class { async detect() { return [{ rawValue: window.__payload }]; } };
      window.__realPlay = HTMLMediaElement.prototype.play;
      HTMLMediaElement.prototype.play = function () {
        return window.__cameraMode === 'video' ? window.__realPlay.call(this) : Promise.resolve();
      };
    }, payload);
    const page = await context.newPage();
    page.setDefaultTimeout(8000);
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '连接我的电脑', exact: true }).first().click();
    await page.getByLabel('电脑节点密码', { exact: true }).fill('previous-node-secret');
    await page.getByRole('button', { name: '扫码连接', exact: true }).click();
    const start = () => page.getByRole('button', { name: '打开相机', exact: true }).click();
    const recognized = () => page.getByText('已识别电脑', { exact: true }).waitFor();
    await start();
    await recognized();
    assert.deepEqual(await page.evaluate(() => window.__cameraCalls.map(c => c.video)), [{ facingMode: { ideal: 'environment' } }, true]);
    pass('unsupported rear-camera constraints fall back to video without requesting audio');

    assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).count(), 0);
    await page.getByRole('button', { name: '手动输入地址', exact: true }).click();
    assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).inputValue(), '');
    await page.getByLabel('电脑节点密码', { exact: true }).fill('previous-node-secret');
    await page.getByRole('button', { name: '扫码连接', exact: true }).click();
    await start();
    await recognized();
    assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).count(), 0);
    assert.equal(node.requests.length, 0);
    pass('camera recognition clears stale credentials and never connects before confirmation');

    await page.evaluate(() => { window.__cameraMode = 'deny'; window.__cameraCalls = []; });
    await start();
    await page.locator('#scanner-status').filter({ hasText: '未获得相机权限' }).waitFor();
    assert.equal(await page.evaluate(() => window.__cameraCalls.length), 1);
    assert(await page.getByRole('button', { name: '打开相机', exact: true }).isEnabled());
    pass('permission denial is actionable and never triggers another permission request');

    await page.evaluate(() => { window.__cameraMode = 'error'; window.__cameraCalls = []; });
    await start();
    await page.locator('#scanner-status').filter({ hasText: '无法打开相机' }).waitFor();
    assert.equal(await page.evaluate(() => window.__cameraCalls.length), 1);
    assert(await page.getByRole('button', { name: '打开相机', exact: true }).isEnabled());
    assert.equal(node.requests.length, 0);
    pass('camera device errors preserve an actionable retry without connection I/O');

    await page.evaluate(() => { window.__cameraMode = 'pending'; window.__fastTimeout = true; });
    const stopped = await page.evaluate(() => window.__stopped);
    await start();
    await page.locator('#scanner-status').filter({ hasText: '超时' }).waitFor();
    await page.evaluate(() => { window.__resolveCamera(window.__fakeStream()); });
    await page.waitForFunction(count => window.__stopped > count, stopped);
    assert(await page.locator('#device-camera').evaluate(video => !video.srcObject));
    pass('camera initialization times out, resets the UI and stops a late permission stream');

    // Draw an actual generated QR into a canvas-backed video stream, without
    // BarcodeDetector or patching the decoder/canvas APIs used by the app.
    await page.addScriptTag({ content: fs.readFileSync('assets/vendor/qrcode.js', 'utf8') });
    const qrPng = await page.evaluate(async () => {
      const q = qrcode(0, 'M'); q.addData(window.__payload, 'Byte'); q.make();
      const image = new Image();
      image.src = 'data:image/svg+xml;base64,' + btoa(q.createSvgTag({ cellSize: 8, margin: 32 }));
      await image.decode();
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
      const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0);
      window.__videoCanvas = canvas;
      window.__videoStream = canvas.captureStream(10);
      window.__videoInterval = setInterval(() => ctx.drawImage(image, 0, 0), 100);
      window.BarcodeDetector = undefined;
      window.__cameraMode = 'video'; window.__fastTimeout = false;
      return canvas.toDataURL('image/png').split(',')[1];
    });
    await start();
    await recognized();
    await page.locator('.scanned-computer code').filter({ hasText: address }).waitFor();
    assert(await page.evaluate(() => window.__videoStream.getTracks().every(track => track.readyState === 'ended')));
    pass('retry after denial, device error and timeout decodes live QR video through bundled jsQR and releases real media tracks');

    await page.getByRole('button', { name: '手动输入地址', exact: true }).click();
    assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).inputValue(), '');
    await page.getByLabel('电脑节点密码', { exact: true }).fill('previous-album-secret');
    await page.getByRole('button', { name: '扫码连接', exact: true }).click();
    await page.locator('#device-qr-image').setInputFiles({ name: 'computer.png', mimeType: 'image/png', buffer: Buffer.from(qrPng, 'base64') });
    await page.getByText('已读取图片地址，请核对后连接。', { exact: true }).waitFor();
    assert.equal(node.requests.length, 0);
    assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).count(), 0);
    await page.getByRole('button', { name: '连接这台电脑', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('#phone-manual-connection input[readonly]'));
    await page.getByRole('button', { name: '连接这台电脑', exact: true }).waitFor();
    const target = page.getByLabel('电脑节点地址', { exact: true });
    assert.equal(await target.inputValue(), address);
    assert.equal(await target.evaluate(input => input.readOnly), true);
    assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).inputValue(), '');
    assert(node.requests.some(request => request.path === '/api/v1/health'));
    assert(!node.requests.some(request => request.path === '/api/v1/projects' && request.authenticated));
    pass('album recognition clears scoped credentials; only confirmation connects and opens the shared readonly-target password form');
    await page.getByLabel('电脑节点密码', { exact: true }).fill('test-only');
    await page.getByLabel('电脑节点密码', { exact: true }).press('Enter');
    await page.locator('.phone-current-node [role=status]').filter({ hasText: '在线' }).waitFor();
    assert(node.requests.some(request => request.path === '/api/v1/projects'));
    assert(node.requests.some(request => request.path === '/api/v1/sessions'));
    if (await page.locator('#phone-manual-connection:visible').count()) {
      assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).inputValue(), '');
    } else {
      await page.getByRole('button', { name: '连接其他电脑', exact: true }).click();
      assert.equal(await page.getByLabel('电脑节点密码', { exact: true }).inputValue(), '');
    }
    assert.equal(await page.locator('#scanned-node-token').count(), 0);
    pass('Enter in the shared manual password form authenticates and loads projects and sessions, then clears the password');
    await page.getByRole('button', { name: '扫码连接', exact: true }).click();

    await page.evaluate(() => { clearInterval(window.__videoInterval); window.__resolveCamera = null; window.__cameraMode = 'pending'; window.__fastTimeout = false; });
    await start();
    await page.waitForFunction(() => window.__rpiWorkCamera && typeof window.__resolveCamera === 'function');
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.waitForFunction(() => !window.__rpiWorkCamera);
    const beforeLeave = await page.evaluate(() => window.__stopped);
    await page.evaluate(() => window.__resolveCamera(window.__fakeStream()));
    await page.waitForFunction(count => window.__stopped > count, beforeLeave);
    pass('leaving settings cancels camera startup and releases late streams');
    assert(!node.requests.some(request => ['prompt', 'queue_message'].includes(request.body?.type) || ['prompt', 'queue_message'].includes(request.body?.command)));
    pass('scanner and password actions never send model messages');
    assert.deepEqual(errors, []);
    fs.rmSync('artifacts/mobile-scanner-blocked.json', { force: true });
    completed = true;
  } finally {
    fs.writeFileSync('artifacts/mobile-scanner-validation.json', JSON.stringify({ scope: 'Formal Dioxus WASM with real canvas QR video frames; simulated camera permissions, isolated HTTP password-authenticated node; no physical camera', status: completed ? 'passed' : 'failed', passCount: results.length, failureCount: completed ? 0 : 1, results, errors }, null, 2));
    await browser.close(); await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
