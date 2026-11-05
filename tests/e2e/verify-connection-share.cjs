// Formal Dioxus WASM: share the connected node from Web and mobile layouts.
// Random-port fixtures only. Clipboard is simulated; QR is actually decoded.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const first = await startFixture(), second = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const firstAddress = 'http://192.168.50.22:3031', secondAddress = 'http://192.168.50.23:3031';
  const results = [], errors = [];
  const pass = name => { results.push(name); console.log('PASS', name); };
  const payload = address => ({ v: 1, type: 'rpi-work-node-address', address });
  const mutations = () => [...first.requests, ...second.requests].filter(r => !['GET', 'OPTIONS'].includes(r.method)).length;
  let desktop, phone;
  const create = async mobile => {
    const context = await browser.newContext(mobile
      ? { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true }
      : { viewport: { width: 1440, height: 1000 } });
    for (const [address, node] of [[firstAddress, first], [secondAddress, second]]) {
      await context.route(`${address}/**`, async route => {
        const response = await route.fetch({ url: new URL(new URL(route.request().url()).pathname, node.url).href });
        await route.fulfill({ response });
      });
    }
    await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await context.addInitScript(() => {
      window.__copied = []; window.__copyMode = 'ok';
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async value => {
        if (window.__copyMode === 'fail') throw new Error('fixture clipboard denied');
        window.__copied.push(value);
        if (window.__copyMode === 'pending') await new Promise(resolve => { window.__finishCopy = resolve; });
      } } });
    });
    const page = await context.newPage(); page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    if (mobile) await page.getByRole('button', { name: '连接我的电脑', exact: true }).first().click();
    else {
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
      await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    }
    await page.addScriptTag({ content: fs.readFileSync('assets/vendor/jsQR.js', 'utf8') });
    return page;
  };
  const share = page => page.getByRole('button', { name: '分享当前连接', exact: true });
  const dialog = page => page.getByRole('dialog', { name: '分享当前连接', exact: true });
  const exported = page => dialog(page).locator('#device-address-payload').inputValue().then(JSON.parse);
  const openShare = async page => { await share(page).click(); await dialog(page).locator('#device-address-qr-code svg').waitFor(); };
  const decodeQr = async page => JSON.parse(await dialog(page).locator('#device-address-qr-code svg').evaluate(async svg => {
    const clone = svg.cloneNode(true); clone.setAttribute('width', '512'); clone.setAttribute('height', '512');
    const url = URL.createObjectURL(new Blob([clone.outerHTML], { type: 'image/svg+xml' }));
    try {
      const image = new Image(); image.src = url; await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = 512;
      const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0, 512, 512);
      const pixels = ctx.getImageData(0, 0, 512, 512);
      return window.jsQR(pixels.data, 512, 512)?.data || '';
    } finally { URL.revokeObjectURL(url); }
  }));
  try {
    desktop = await create(false);
    assert.equal(await share(desktop).count(), 0);
    await desktop.getByRole('button', { name: '连接其他节点', exact: true }).click();
    const address = desktop.getByLabel('节点地址', { exact: true });
    const connect = desktop.getByRole('button', { name: '连接节点', exact: true });
    await address.fill(firstAddress); await desktop.getByLabel('节点密码', { exact: true }).fill('test-only');
    await connect.click(); await desktop.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    const beforeSharing = mutations();
    await address.fill(secondAddress); // This draft is deliberately NOT connected.
    await openShare(desktop);
    assert.deepEqual(await exported(desktop), payload(firstAddress));
    assert.deepEqual(await decodeQr(desktop), payload(firstAddress));
    assert.match(await dialog(desktop).innerText(), /不包含密码/);
    assert.equal(await dialog(desktop).locator('input[type=password]').count(), 0);
    pass('Web shares the current connection, not the edited draft, using a real address-only QR');

    await dialog(desktop).getByRole('button', { name: '复制地址', exact: true }).click();
    await dialog(desktop).getByRole('status').filter({ hasText: '已复制连接地址' }).waitFor();
    await dialog(desktop).getByRole('button', { name: '复制连接信息', exact: true }).click();
    await dialog(desktop).getByRole('status').filter({ hasText: '已复制连接信息' }).waitFor();
    const copies = await desktop.evaluate(() => window.__copied);
    assert.equal(copies[0], firstAddress); assert.deepEqual(JSON.parse(copies[1]), payload(firstAddress));
    assert(!copies.some(value => /test-only|password|token|cookie/i.test(value)));
    pass('copy address and versioned connection data never include a password, Token or Cookie');

    await desktop.evaluate(() => { window.__copyMode = 'fail'; });
    await dialog(desktop).getByRole('button', { name: '复制连接信息', exact: true }).click();
    await dialog(desktop).getByRole('status').filter({ hasText: '请手动复制' }).waitFor();
    assert.deepEqual(await exported(desktop), payload(firstAddress));
    pass('clipboard denial leaves selectable connection data and clear manual-copy feedback');

    const shareAddress = dialog(desktop).getByLabel('对方可访问的节点地址', { exact: true });
    await shareAddress.fill(secondAddress);
    await dialog(desktop).getByRole('button', { name: '重新生成', exact: true }).click();
    await dialog(desktop).locator('#device-address-qr-code svg').waitFor();
    assert.deepEqual(await exported(desktop), payload(secondAddress));
    assert.deepEqual(await decodeQr(desktop), payload(secondAddress));
    assert.match(await desktop.locator('.node-address').innerText(), /192\.168\.50\.22/);
    assert.equal(mutations(), beforeSharing);
    pass('editing the address and Regenerate replaces the QR without switching or mutating the node');
    for (const invalid of ['', '192.168.50.22:3031', 'http://', 'http://127.0.0.1:3031', 'javascript:alert(1)', 'https://user:password@example.test', 'https://example.test/path', 'https://example.test/?password=secret']) {
      await shareAddress.fill(invalid);
      await dialog(desktop).getByRole('alert').filter({ hasText: /./ }).waitFor();
      await dialog(desktop).getByRole('status').filter({ hasText: '配置可达地址后生成二维码' }).waitFor();
      if (!invalid) assert.match(await dialog(desktop).getByRole('alert').innerText(), /尚未配置可分享地址.*无法生成二维码/);
      assert.equal(await dialog(desktop).locator('#device-address-qr-code svg').count(), 0);
      assert.equal(await dialog(desktop).locator('#device-address-payload').inputValue(), '');
      assert(await dialog(desktop).getByRole('button', { name: '重新生成', exact: true }).isDisabled());
      assert(await dialog(desktop).getByRole('button', { name: '复制地址', exact: true }).isDisabled());
      assert(await dialog(desktop).getByRole('button', { name: '复制连接信息', exact: true }).isDisabled());
    }
    assert.equal(mutations(), beforeSharing);
    pass('empty, malformed and loopback share addresses explain the missing QR with no copied data or node mutation');

    await shareAddress.fill(firstAddress);
    await desktop.evaluate(() => { window.__copyMode = 'pending'; });
    await dialog(desktop).getByRole('button', { name: '复制地址', exact: true }).click();
    await desktop.waitForFunction(() => typeof window.__finishCopy === 'function');
    await desktop.keyboard.press('Escape');
    await connect.click(); await desktop.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await openShare(desktop);
    assert.deepEqual(await exported(desktop), payload(secondAddress));
    await desktop.evaluate(() => { window.__finishCopy(); window.__copyMode = 'ok'; });
    await desktop.waitForTimeout(100);
    assert.equal(await dialog(desktop).getByRole('status').innerText(), '');
    await desktop.screenshot({ path: 'artifacts/connection-share-web.png', animations: 'disabled' });
    pass('switching nodes resets the share source and ignores late clipboard feedback');

    await dialog(desktop).getByRole('button', { name: '完成', exact: true }).click();
    await address.fill(secondAddress); await connect.click();
    await openShare(desktop); // Must close if a same-address connection epoch changes.
    await desktop.evaluate(() => document.querySelector('.remote-node-form button').click());
    await dialog(desktop).waitFor({ state: 'hidden' });
    await desktop.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await openShare(desktop);
    assert.deepEqual(await exported(desktop), payload(secondAddress));
    await desktop.keyboard.press('Escape');
    assert(await share(desktop).evaluate(button => document.activeElement === button));
    pass('reconnecting the same node closes the previous dialog and Escape restores the share trigger');

    phone = await create(true);
    assert.equal(await share(phone).count(), 0);
    await phone.getByRole('button', { name: '手动输入地址', exact: true }).click();
    await phone.getByLabel('电脑节点地址', { exact: true }).fill(firstAddress);
    await phone.getByRole('button', { name: '连接电脑', exact: true }).click();
    await phone.locator('.phone-current-node [role=status]').filter({ hasText: '在线' }).waitFor();
    await phone.getByLabel('电脑节点地址', { exact: true }).fill(secondAddress);
    const beforePhoneShare = mutations();
    await openShare(phone);
    assert.deepEqual(await exported(phone), payload(firstAddress));
    assert.deepEqual(await decodeQr(phone), payload(firstAddress));
    pass('mobile exposes Share current connection and excludes its unsubmitted manual address draft');

    for (const width of [390, 320]) {
      for (const mode of ['light', 'dark']) {
        await phone.setViewportSize({ width, height: 844 });
        await phone.evaluate(mode => { document.documentElement.dataset.mode = mode; }, mode);
        // Responsive mode reset may close the dialog; reopen from the live connection.
        if (!await dialog(phone).isVisible()) await openShare(phone);
        await dialog(phone).getByRole('button', { name: '复制地址', exact: true }).scrollIntoViewIfNeeded();
        assert(await phone.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
        assert(await dialog(phone).evaluate(el => el.scrollWidth <= el.clientWidth));
        const box = await dialog(phone).getByRole('button', { name: '复制地址', exact: true }).boundingBox();
        await phone.screenshot({ path: `artifacts/connection-share-phone-${width}-${mode}.png`, animations: 'disabled' });
        assert(box.height >= 44 && box.x >= 0 && box.x + box.width <= width, JSON.stringify({ width, mode, box, metrics: await phone.evaluate(() => ({ innerWidth, scale: visualViewport.scale })) }));
      }
    }
    await phone.evaluate(() => { window.__copyMode = 'ok'; });
    await dialog(phone).getByRole('button', { name: '复制连接信息', exact: true }).click();
    await dialog(phone).getByRole('status').filter({ hasText: '已复制连接信息' }).waitFor();
    assert.deepEqual(JSON.parse(await phone.evaluate(() => window.__copied.at(-1))), payload(firstAddress));
    assert.equal(mutations(), beforePhoneShare);
    assert.deepEqual(errors, []);
    pass('390/320 light/dark mobile sharing stays bounded with 44px targets and no workspace mutations');
  } catch (error) {
    if (desktop) await desktop.screenshot({ path: 'artifacts/connection-share-failure.png' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/connection-share-validation.json', JSON.stringify({ scope: 'Formal WASM; isolated HTTP fixtures; actual QR decode; clipboard simulated; no physical phone', results, errors }, null, 2));
    await browser.close(); await first.close(); await second.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
