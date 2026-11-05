// Real WASM + isolated HTTP/SSE fixture. Build/serve the integrated UI separately.
// Run: node tests/e2e/verify-composer-picker-popovers.cjs
// Browsing only: no model/thinking setters, prompts, credentials or config writes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const CURRENT = 'deepseek/deepseek-v4-pro';
const DRAFT = '锚定菜单回归：保留草稿 😀';
const LONG = '超长模型名称 · '.repeat(35);
const LABELS = { off: '关闭', minimal: '最低', low: '低', medium: '中', high: '高', xhigh: '更高', max: '最高' };
const catalog = () => [
  { id: CURRENT, name: 'Custom current', provider: 'deepseek', reasoning: true },
  { id: 'alpha/first', name: 'Keyboard 01 enabled', provider: 'alpha', reasoning: true },
  { id: 'ambiguous', name: 'Keyboard 02 disabled', provider: 'alpha', reasoning: true },
  { id: 'alpha/last', name: 'Keyboard 03 enabled', provider: 'alpha', reasoning: true },
  { id: 'ambiguous', name: 'Duplicate identity', provider: 'beta', reasoning: false },
  { id: `long/${'raw-model-'.repeat(30)}`, name: LONG, provider: 'long', reasoning: true },
];
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, message) {
  const end = Date.now() + 10000;
  while (Date.now() < end) { if (await check()) return; await delay(30); }
  throw new Error(`Timed out: ${message}`);
}
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], geometry = [], errors = [], commands = [], external = [], runtime = [];
  let fixture, browser, context, page;
  const pass = message => { results.push(message); console.log('PASS', message); };
  try {
    fixture = await startFixture(0);
    fixture.controls.models = catalog();
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    const allowed = [new URL(UI).origin, new URL(fixture.url).origin];
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {} });
      if (!allowed.includes(url.origin)) { external.push(request.url()); return route.abort('blockedbyclient'); }
      if (url.origin !== allowed[1] || request.method() !== 'POST' || !url.pathname.endsWith('/commands')) return route.continue();
      const body = request.postDataJSON(); commands.push({ path: url.pathname, body });
      // Fail closed before a regression can mutate even the isolated node.
      if (!body.type?.startsWith('get_')) {
        errors.push(`Forbidden command: ${body.type}`);
        return route.fulfill({ status: 200, json: { type: 'response', command: body.type, id: body.id ?? null, success: false, data: null, error: 'Popover regression forbids all writes' } });
      }
      if (body.type === 'get_available_thinking_levels') {
        // Extend only this browser's discovery response to exercise all seven
        // standard labels. Authoritative fixture runtime remains genuinely high.
        const response = await route.fetch(), snapshot = await response.json();
        snapshot.data.levels = Object.keys(LABELS);
        return route.fulfill({ response, json: snapshot });
      }
      if (body.type === 'get_state' && url.pathname.includes('/dark/')) {
        const response = await route.fetch(), snapshot = await response.json();
        runtime.push(snapshot.data);
        return route.fulfill({ response, json: snapshot });
      }
      return route.continue();
    });
    await page.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard', toolRendering: 'simple', customThemes: [] })));
    const model = () => page.getByRole('button', { name: '选择供应商与模型', exact: true });
    const thinking = () => page.getByRole('button', { name: '思考等级', exact: true });
    const modelDialog = () => page.getByRole('dialog', { name: '选择供应商与模型', exact: true });
    const thinkingDialog = () => page.getByRole('dialog', { name: '选择思考等级', exact: true });
    const field = () => page.locator('.composer-main textarea');
    const send = () => page.getByRole('button', { name: '发送消息', exact: true });
    const search = () => modelDialog().getByRole('searchbox', { name: '搜索供应商或模型', exact: true });
    const option = level => thinkingDialog().locator(`.composer-thinking-option[data-thinking-level="${level}"]`);
    const focused = locator => locator.evaluate(element => element === document.activeElement);
    async function open(trigger, dialog) {
      await trigger().click(); await dialog().waitFor();
      await until(async () => await trigger().getAttribute('aria-expanded') === 'true', 'trigger expanded');
    }
    async function escape(trigger, dialog) {
      await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' });
      await until(async () => await trigger().getAttribute('aria-expanded') === 'false' && await focused(trigger()), 'Escape restores originating trigger');
    }
    async function refresh() {
      const before = commands.filter(item => item.body.type === 'get_available_models').length;
      await modelDialog().getByRole('button', { name: '刷新模型列表', exact: true }).click();
      await until(() => commands.filter(item => item.body.type === 'get_available_models').length > before, 'explicit catalog refresh');
    }
    async function noModal(dialog) {
      const state = await dialog().evaluate(element => ({
        popover: element.classList.contains('composer-picker-popover'),
        modal: element.matches(':modal'), ariaModal: element.getAttribute('aria-modal'),
        inert: !!document.querySelector('.composer-main textarea')?.closest('[inert]'),
        modals: document.querySelectorAll(':modal, [aria-modal="true"]').length,
        backdrops: [...document.querySelectorAll('.modal-backdrop, .dialog-backdrop, [data-backdrop]')].filter(e => e.getClientRects().length).length,
      }));
      assert(state.popover && !state.modal && state.ariaModal !== 'true' && !state.inert && !state.modals && !state.backdrops, JSON.stringify(state));
    }
    async function settled(dialog) {
      await dialog().evaluate(async element => { await Promise.all(element.getAnimations({ subtree: true }).map(animation => animation.finished.catch(() => {}))); });
    }
    async function measure(dialog, trigger, expectedWidth, mode, width, picker) {
      await settled(dialog);
      const size = await dialog().evaluate((element, triggerId) => {
        const button = document.querySelector(triggerId), r = element.getBoundingClientRect(), b = button.getBoundingClientRect();
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height,
          anchor: { left: b.left, right: b.right, top: b.top, bottom: b.bottom },
          scroll: element.scrollWidth, client: element.clientWidth, pageScroll: document.documentElement.scrollWidth,
          viewportHeight: innerHeight };
      }, picker === 'model' ? '.composer-model-picker' : '.composer-thinking-select');
      geometry.push({ mode, viewport: width, picker, ...size });
      assert(size.left >= 7 && size.right <= width - 7 && size.top >= 7 && size.bottom <= size.viewportHeight - 7, `safe viewport: ${JSON.stringify(size)}`);
      assert(Math.abs(size.width - Math.min(expectedWidth, width - 16)) <= 2, `${picker} preferred width: ${JSON.stringify(size)}`);
      assert(size.height <= 421, `420px maximum height: ${JSON.stringify(size)}`);
      assert(size.bottom <= size.anchor.top + 1 && size.anchor.top - size.bottom <= 20, `anchored immediately above button, not centered: ${JSON.stringify(size)}`);
      assert(size.left < size.anchor.right && size.right > size.anchor.left, 'popover horizontally overlaps its trigger');
      assert(Math.abs((size.top + size.bottom) / 2 - size.viewportHeight / 2) > 60, 'popover must not be viewport-centered');
      assert(size.scroll <= size.client + 1 && size.pageScroll <= width + 1, `no horizontal overflow: ${JSON.stringify(size)}`);
      assert(await trigger().isVisible());
      await noModal(dialog);
    }
    async function composerFits(width) {
      // Desktop-to-mobile resizes animate the sidebar offscreen; measure hit targets
      // only after that real transition ends, not while the departing rail overlaps.
      await page.locator('.sidebar').evaluate(async e => {
        await Promise.all(e.getAnimations().map(animation => animation.finished.catch(() => {})));
      });
      const rects = await Promise.all([model(), thinking(), send()].map(locator => locator.evaluate(e => {
        const r = e.getBoundingClientRect(), s = getComputedStyle(e);
        const x = Math.max(0, Math.min(innerWidth - 1, r.left + r.width / 2)), y = r.top + r.height / 2;
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height,
          visible: s.visibility !== 'hidden' && s.display !== 'none', hit: e.contains(document.elementFromPoint(x, y)), hitElement: document.elementFromPoint(x, y)?.outerHTML.slice(0, 300) };
      })));
      for (const r of rects) assert(r.visible && r.hit && r.width > 0 && r.height > 0 && r.left >= 0 && r.right <= width + 1, `composer control remains usable: ${JSON.stringify(rects)}`);
      for (let i = 0; i < rects.length; i++) for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i], b = rects[j];
        assert(Math.min(a.right, b.right) - Math.max(a.left, b.left) <= 1 || Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) <= 1, 'model, thinking and send must not overlap');
      }
      assert.equal(await field().inputValue(), DRAFT);
    }
    async function mobileTargets(dialog, selector) {
      for (const target of await dialog().locator(selector).all()) {
        if (!await target.isVisible()) continue;
        await target.scrollIntoViewIfNeeded();
        const box = await target.boundingBox();
        assert(box && box.height >= 43.5 && box.width >= 43.5, `44px mobile target: ${JSON.stringify(box)}`);
      }
    }

    await page.goto(UI);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.waitForFunction(() => { const e = document.querySelector('.composer-main textarea'); return e && !e.disabled && e.dataset.imeReady === 'true'; });
    await field().fill(DRAFT);
    await until(async () => (await model().innerText()).trim() === 'Custom current' && !await thinking().isDisabled(), 'runtime-matched catalog name and thinking discovery');
    assert.equal(await model().getAttribute('title'), `deepseek / ${CURRENT}`, 'tooltip preserves provider and raw model ID');
    assert.equal(await thinking().getAttribute('data-thinking-level'), 'high');
    assert.equal((await thinking().innerText()).trim(), '高');
    await until(() => runtime.some(state => state.modelId === CURRENT && state.thinkingLevel === 'high'), 'genuine fixture current model/high readback');
    pass('current catalog name is real; raw model tooltip and authoritative high/高 remain distinct');

    await open(model, modelDialog);
    assert.equal(await modelDialog().getAttribute('id'), 'composer-model-picker');
    await measure(modelDialog, model, 320, 'light', 1280, 'model');
    // Native pointer clicks, never force:true: a modal/backdrop would block this.
    await thinking().click(); await thinkingDialog().waitFor(); await modelDialog().waitFor({ state: 'hidden' });
    await until(async () => await model().getAttribute('aria-expanded') === 'false', 'switch closes previous picker');
    assert.equal(await thinkingDialog().getAttribute('id'), 'composer-thinking-picker');
    await noModal(thinkingDialog);
    assert.equal(await page.locator('.composer-picker-popover').count(), 1);
    assert.equal(await thinkingDialog().locator('[aria-pressed="true"]').count(), 1);
    for (const [raw, label] of Object.entries(LABELS)) {
      assert.equal((await option(raw).locator('.composer-model-label').innerText()).trim(), label);
      assert.equal(await option(raw).getAttribute('aria-pressed'), String(raw === 'high'));
    }
    await field().click(); await thinkingDialog().waitFor({ state: 'hidden' });
    await until(() => focused(field()), 'outside input click retains input focus');
    await open(model, modelDialog); await field().click(); await modelDialog().waitFor({ state: 'hidden' });
    await until(() => focused(field()), 'model outside click does not steal textarea focus');
    await open(model, modelDialog); await escape(model, modelDialog);
    await open(thinking, thinkingDialog); await escape(thinking, thinkingDialog);
    pass('direct picker switching, non-modal/inert-free browsing, outside input focus and Escape restoration');

    await open(model, modelDialog); await search().fill('Keyboard');
    await until(async () => await modelDialog().locator('.composer-model-option').count() === 3, 'filtered keyboard fixture');
    const first = modelDialog().locator('[data-model-id="alpha/first"]');
    const disabled = modelDialog().locator('[data-model-id="ambiguous"]');
    const last = modelDialog().locator('[data-model-id="alpha/last"]');
    await until(async () => !await first.isDisabled() && !await last.isDisabled(), 'model discovery settled');
    assert(await disabled.isDisabled(), 'cross-provider duplicate is naturally disabled');
    await first.focus(); await page.keyboard.press('ArrowDown'); await until(() => focused(last), 'ArrowDown skips disabled middle option');
    await page.keyboard.press('ArrowUp'); await until(() => focused(first), 'ArrowUp skips disabled middle option');
    await page.keyboard.press('End'); await until(() => focused(last), 'End reaches last enabled model');
    await page.keyboard.press('Home'); await until(() => focused(first), 'Home reaches first enabled model');
    await last.focus(); await page.keyboard.press('Tab'); await modelDialog().waitFor({ state: 'hidden' });
    assert(!await page.evaluate(() => !!document.activeElement?.closest('.composer-picker-popover')), 'model Tab leaves rather than traps focus');
    await open(thinking, thinkingDialog);
    await option('high').focus(); await page.keyboard.press('ArrowDown'); await until(() => focused(option('xhigh')), 'thinking ArrowDown');
    await page.keyboard.press('ArrowUp'); await until(() => focused(option('high')), 'thinking ArrowUp');
    await page.keyboard.press('Home'); await until(() => focused(option('off')), 'thinking Home');
    await page.keyboard.press('End'); await until(() => focused(option('max')), 'thinking End');
    await page.keyboard.press('Tab'); await thinkingDialog().waitFor({ state: 'hidden' });
    assert(!await page.evaluate(() => !!document.activeElement?.closest('.composer-picker-popover')), 'Tab leaves rather than traps focus');
    assert.equal(await thinking().getAttribute('data-thinking-level'), 'high', 'keyboard navigation never commits');
    await open(thinking, thinkingDialog); await option('high').click();
    await thinkingDialog().waitFor({ state: 'hidden' });
    await until(() => focused(thinking()), 'clicking current raw high closes and restores trigger without a setter');
    pass('keyboard arrows skip disabled models; Home/End navigate both menus; Tab leaves and closes without writes');

    // A catalog miss must show the raw runtime ID, not a fabricated provider label.
    fixture.controls.models = catalog().filter(item => item.id !== CURRENT);
    await open(model, modelDialog); await refresh();
    await until(async () => (await model().innerText()).trim() === CURRENT, 'raw ID fallback when catalog lacks current runtime');
    assert.equal(await model().getAttribute('title'), `deepseek / ${CURRENT}`);
    await escape(model, modelDialog);
    fixture.controls.models = catalog().map(item => item.id === CURRENT ? { ...item, name: LONG } : item);
    await open(model, modelDialog); await refresh();
    await until(async () => (await model().innerText()).trim() === LONG.trim(), 'long current name discovered without setter');
    await escape(model, modelDialog);
    pass('catalog miss falls back to raw ID; long current name is displayed without mutating runtime');

    for (const mode of ['light', 'dark']) {
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.getByRole('button', { name: '更多操作', exact: true }).click();
      await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(mode);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      await appearance.waitFor({ state: 'hidden' });
      for (const width of [1280, 390, 320]) {
        await page.setViewportSize({ width, height: 900 });
        await composerFits(width);
        await open(model, modelDialog); await search().fill('');
        // Open all providers through their native disclosure controls, including
        // the very long ID/name option. No DOM style or geometry is fabricated.
        for (const group of await modelDialog().locator('.composer-provider-group').all()) {
          if (!await group.evaluate(e => e.open)) await group.locator('summary').click();
        }
        await measure(modelDialog, model, 320, mode, width, 'model');
        if (width <= 390) await mobileTargets(modelDialog, '.composer-model-option');
        await page.screenshot({ path: `artifacts/composer-picker-popovers-${mode}-${width}-model.png`, animations: 'disabled' });
        await escape(model, modelDialog); await composerFits(width);
        await open(thinking, thinkingDialog);
        await measure(thinkingDialog, thinking, 224, mode, width, 'thinking');
        assert.equal(await option('high').getAttribute('aria-pressed'), 'true');
        assert.equal(await thinkingDialog().locator('.composer-thinking-option[aria-pressed="true"]').count(), 1);
        if (width <= 390) await mobileTargets(thinkingDialog, '.composer-thinking-option');
        await option('high').focus();
        await page.screenshot({ path: `artifacts/composer-picker-popovers-${mode}-${width}-thinking.png`, animations: 'disabled' });
        await escape(thinking, thinkingDialog); await composerFits(width);
      }
      pass(`${mode}: 1280/390/320 safe anchored 320/224px popovers, max-height 420, long names, usable thinking/send and mobile 44px options`);
    }
    // Explicitly emulate an older WebView without the Popover API. This changes
    // capability only; positions and focus still come from the product code.
    await page.evaluate(() => Object.defineProperty(HTMLElement.prototype, 'showPopover', { configurable: true, value: undefined }));
    await open(model, modelDialog);
    assert.equal(await modelDialog().getAttribute('popover'), null);
    await search().fill('Keyboard');
    const fallbackLast = modelDialog().locator('[data-model-id="alpha/last"]');
    await fallbackLast.focus(); await page.keyboard.press('Tab');
    await modelDialog().waitFor({ state: 'hidden' });
    await until(() => focused(thinking()), 'fallback Tab proceeds to the next composer control, not a hidden menu or body');
    await open(thinking, thinkingDialog);
    // On 320px the menu legitimately overlaps the textarea center; click its
    // exposed left edge, which is outside the menu, rather than force a hit.
    await field().click({ position: { x: 8, y: 8 } }); await thinkingDialog().waitFor({ state: 'hidden' });
    assert(await focused(field()));
    await open(thinking, thinkingDialog); await escape(thinking, thinkingDialog);
    assert.equal(await page.evaluate(() => window.__rpiComposerPickers.size), 0, 'all observers and picker instances disposed');
    pass('older-WebView fallback hides synchronously, supports Tab/Escape/outside focus and cleans up instances');

    assert.equal(await field().inputValue(), DRAFT);
    assert.equal(await thinking().getAttribute('data-thinking-level'), 'high');
    assert.equal((await thinking().innerText()).trim(), '高');
    assert(runtime.length && runtime.every(state => state.modelId === CURRENT && state.thinkingLevel === 'high'));
    assert.deepEqual(commands.filter(item => !item.body.type.startsWith('get_')), [], 'zero attempted prompt/config/credential writes');
    assert.deepEqual(fixture.requests.filter(item => item.body?.type && !item.body.type.startsWith('get_')), [], 'zero actual fixture writes');
    assert.deepEqual(external, [], 'no provider/external traffic');
    assert.deepEqual(errors, [], 'no application errors or blocked write attempts');
    pass('draft and authoritative runtime preserved; zero prompts, credentials, config writes or external traffic');
  } catch (error) {
    errors.push(error.stack || String(error));
    if (page) await page.screenshot({ path: 'artifacts/composer-picker-popovers-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/composer-picker-popovers-validation.json', JSON.stringify({ results, geometry, commands, runtime, external, errors }, null, 2));
    if (browser) await browser.close();
    if (fixture) await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
