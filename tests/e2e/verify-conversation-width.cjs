// Run against already-served formal WASM assets. Isolated HTTP/SSE fixture only;
// this script never builds assets, submits a prompt, or writes credentials.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture, fixtureCommand } = require('../fixtures/ui-fixture.cjs');
const WEB = process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/';
const DRAFT = '只保留本地草稿，不发送。';
const QUEUE_TEXT = '隔离队列宽度验证：' + 'Please retain this readable queued follow-up while comparing the message column, composer and queue across desktop and mobile layouts. '.repeat(6);

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  fixture.setMessages([
    { entryId: 'width-user', role: 'user', content: '验证宽屏消息与手机安全边距。' },
    { entryId: 'width-answer', role: 'assistant', content: [{ type: 'text', text: '# 宽度验证\n\n正式 Markdown 水平分隔线。\n\n---\n\n可换行的正文与长路径：' + 'fixture/width/'.repeat(12) }] },
  ]);
  // Seed only: never click enqueue/cancel/clear or submit the main composer.
  fixture.setQueue([{ entryId: 'width-queue', queue: 'followUp', status: 'queued', text: QUEUE_TEXT }]);
  const results = [], geometry = [], errors = [], blockedRequests = [];
  let browser, page, activeCase;
  const pass = text => { results.push(text); console.log('PASS', text); };
  const near = (actual, expected, name) => assert(Math.abs(actual - expected) <= 1, `${name}: ${actual} != ${expected}`);
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1920, height: 1000 } });
    page = await context.newPage();
    page.setDefaultTimeout(15000);
    page.on('pageerror', error => errors.push(error.message));
    const webOrigin = new URL(WEB).origin, fixtureOrigin = new URL(fixture.url).origin;
    // Even formal-server bootstrap API requests go to the isolated fixture.
    // Fail closed on API writes and unknown origins; only formal static assets
    // and fixture reads are permitted. WORK_UI_URL may include an /app/ prefix.
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      const apiStart = url.pathname.indexOf('/api/v1/');
      if (apiStart >= 0) {
        let body = {};
        try { body = request.postDataJSON() || {}; } catch { /* rejected below */ }
        const read = ['GET', 'HEAD', 'OPTIONS'].includes(request.method()) ||
          (request.method() === 'POST' && url.pathname.endsWith('/open')) ||
          (request.method() === 'POST' && typeof body.type === 'string' && body.type.startsWith('get_'));
        if (![webOrigin, fixtureOrigin].includes(url.origin) || !read) {
          blockedRequests.push({ url: url.href, method: request.method(), type: body.type });
          return route.abort('blockedbyclient');
        }
        if (url.origin === fixtureOrigin) return route.continue();
        const response = await route.fetch({ url: new URL(url.pathname.slice(apiStart) + url.search, fixture.url).href });
        return route.fulfill({ response });
      }
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, body: '{}' });
      if (url.origin === webOrigin && ['GET', 'HEAD'].includes(request.method())) return route.continue();
      blockedRequests.push({ url: url.href, method: request.method() });
      return route.abort('blockedbyclient');
    });
    const queue = page.locator('.composer-main > .message-queue.composer-queue-panel');
    // Measure the real width-bearing row, not its narrower rendered bubble.
    const row = page.locator('.transcript .message-turn[data-history-id="width-answer"]');
    const inspector = page.locator('.inspector');
    const settle = () => page.evaluate(async () => {
      await document.fonts.ready;
      await Promise.race([
        Promise.all(document.getAnimations().filter(a => a.effect?.getTiming().iterations !== Infinity).map(a => a.finished.catch(() => {}))),
        new Promise(resolve => setTimeout(resolve, 250)),
      ]);
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    });
    async function setInspector(open) {
      if (await inspector.isVisible() !== open) {
        if (open) await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
        else await inspector.getByRole('button', { name: '关闭任务上下文', exact: true }).click();
      }
      await inspector.waitFor({ state: open ? 'visible' : 'hidden' });
    }
    const layout = () => page.evaluate(() => {
      const box = selector => {
        const el = document.querySelector(selector);
        if (!el) throw new Error(`Missing layout surface: ${selector}`);
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, height: r.height, width: r.width, center: r.x + r.width / 2, client: el.clientWidth, scroll: el.scrollWidth };
      };
      const transcript = document.querySelector('.transcript'), style = getComputedStyle(transcript);
      const panel = document.querySelector('section.conversation-panel');
      // Resolve tokens including the source's wide-screen clamp().
      const measure = token => {
        const probe = document.createElement('div');
        probe.style.cssText = `position:absolute;visibility:hidden;width:var(${token});height:0`;
        panel.append(probe);
        const width = parseFloat(getComputedStyle(probe).width);
        probe.remove();
        return width;
      };
      return {
        rootMax: parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--conversation-content-max')),
        max: measure('--conversation-content-max'), readingMax: measure('--conversation-reading-max'),
        mode: document.documentElement.dataset.mode,
        gutter: parseFloat(style.paddingLeft), gutterRight: parseFloat(style.paddingRight), transcript: box('.transcript'),
        message: box('.transcript .message-turn[data-history-id="width-answer"]'), ask: box('[data-request-id="width-ask"]'),
        composer: box('.composer-main > .composer'), foot: box('.composer-main > .composer-foot'), stats: box('.composer-main > .runtime-footer'),
        queue: box('.composer-main > .message-queue.composer-queue-panel'), panel: box('section.conversation-panel'),
        containerType: getComputedStyle(document.querySelector('section.conversation-panel')).containerType,
        pageOverflow: document.documentElement.scrollWidth > innerWidth + 1,
      };
    });
    async function checkWidth(theme, width, inspectorState, suffix = '') {
      activeCase = { theme, width, inspector: inspectorState, suffix };
      await page.setViewportSize({ width, height: width <= 700 ? 844 : 1000 });
      if (width >= 1280) await setInspector(inspectorState === 'open');
      else await inspector.waitFor({ state: 'hidden' });
      await row.waitFor({ state: 'visible' });
      await row.scrollIntoViewIfNeeded();
      await settle();
      const m = await layout(), available = m.transcript.client - m.gutter - m.gutterRight;
      const expected = Math.min(m.max, available), readingExpected = Math.min(m.readingMax, available);
      const record = { ...activeCase, layout: m };
      geometry.push(record); // Retain geometry even when an assertion fails.
      assert.equal(m.mode, theme);
      assert.equal(m.rootMax, 1000, 'unmodified root workspace cap');
      const sourceCap = width >= 1440 ? Math.min(1120, Math.max(1000, width * .7)) : 1000;
      near(m.max, sourceCap, 'source workspace cap; no baseline CSS overrides');
      assert(m.containerType.split(' ').includes('inline-size'), 'conversation panel must establish the inline-size container');
      assert(m.message.bottom > m.transcript.y && m.message.y < m.transcript.bottom, 'measure a row actually visible in the transcript');
      near(m.readingMax, m.max, 'current source shares the workspace measure with message rows');
      for (const name of ['message', 'ask', 'composer', 'foot', 'stats', 'queue']) {
        const reading = ['message', 'ask'].includes(name);
        near(m[name].width, reading ? readingExpected : expected, `${width}px ${name} width`);
        near(m[name].center, m.message.center, `${width}px ${name} shares the reading center`);
        if (reading) {
          near(m[name].x, m.message.x, `${width}px ${name} left edge`);
          near(m[name].right, m.message.right, `${width}px ${name} right edge`);
        }
        assert(m[name].x >= 0 && m[name].right <= width + 1, `${width}px ${name} viewport bounds`);
        // Footer metrics intentionally scroll inside their bounded container
        // on extra-narrow safe-area layouts; do not confuse that with page overflow.
        if (name !== 'stats') assert(m[name].scroll <= m[name].client + 1, `${width}px ${name} internal overflow`);
      }
      assert(m.composer.width <= m.max + 1, 'composer respects the source workspace cap');
      near(m.queue.width, m.composer.width, 'inline queue uses composer width');
      near(m.queue.x, m.composer.x, 'inline queue shares composer left edge');
      assert(!m.pageOverflow, `${width}px page overflow`);
      assert(m.transcript.scroll <= m.transcript.client + 1, `${width}px transcript overflow`);
      if (width === 1920 && inspectorState === 'closed') near(m.composer.width, sourceCap, 'large desktop reaches the source cap');
      const stem = `artifacts/conversation-width-${theme}-${width}-${inspectorState}${suffix}`;
      await page.screenshot({ path: `${stem}-main.png`, animations: 'disabled' });
      assert.equal(await queue.getAttribute('role'), 'region');
      assert.equal(await queue.getAttribute('aria-label'), '待处理消息');
      assert.equal(await queue.locator('textarea, dialog, [role="dialog"]').count(), 0, 'no private queue editor or dialog');
      assert.equal(await page.getByRole('dialog', { name: '消息队列', exact: true }).count(), 0);
      assert.equal(await page.getByText(/本地提案|提案编辑/).count(), 0, 'no local proposals');
      const item = queue.locator('.composer-queue-item[data-entry-id="width-queue"]');
      assert.equal(await item.locator('.queue-item-text').textContent(), QUEUE_TEXT);
      const q = await queue.evaluate(el => {
        const box = e => ({ ...e.getBoundingClientRect().toJSON(), client: e.clientWidth, scroll: e.scrollWidth });
        const list = el.querySelector('.composer-queue-list');
        return { ...box(el), inComposer: el.parentElement.matches('.composer-main'), list: {
          ...box(list), maxHeight: parseFloat(getComputedStyle(list).maxHeight), overflowY: getComputedStyle(list).overflowY } };
      });
      record.queue = q;
      assert(q.inComposer, 'queue is inline in the composer');
      near(q.width, m.composer.width, 'queue uses shared composer width');
      assert(q.x >= -1 && q.right <= width + 1, 'queue stays within viewport horizontally');
      assert(q.y >= -1 && q.bottom <= (width <= 700 ? 844 : 1000) + 1, 'queue stays within viewport vertically');
      near(q.list.maxHeight, Math.min((width <= 700 ? 844 : 1000) * .25, 240), 'internal list scroll cap');
      assert(q.list.height <= q.list.maxHeight + 1, 'list is height bounded');
      assert.equal(q.list.overflowY, 'auto');
      assert(q.scroll <= q.client + 1 && q.list.scroll <= q.list.client + 1, 'no queue/list horizontal overflow');
      record.touchActions = [];
      assert.equal(await item.getByRole('button', { name: '立即发送排队消息', exact: true }).count(), 0, 'pending Ask must not permit a second main task');
      for (const name of ['编辑排队消息', '移除排队消息']) {
        const button = item.getByRole('button', { name, exact: true });
        assert(await button.isVisible(), `${name} is visible`);
        const b = await button.boundingBox();
        assert(b && b.x >= q.x - 1 && b.x + b.width <= q.right + 1 && b.y >= q.list.y - 1 && b.y + b.height <= q.list.bottom + 1, 'item action is not clipped');
        assert(await button.evaluate(el => {
          const r = el.getBoundingClientRect();
          return el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
        }), 'item action is not covered');
        if (width <= 700) assert(b.width >= 44 && b.height >= 44, `actual touch dimensions: ${JSON.stringify(b)}`);
        record.touchActions.push(b);
      }
      const options = queue.locator('details.queue-options'), summary = options.locator('summary');
      assert.equal(await summary.textContent(), '队列选项');
      await summary.click(); // Disclosure only; never invoke a queue command.
      for (const name of ['刷新', '清空待执行', '查看历史']) {
        const button = options.getByRole('button', { name, exact: true });
        assert(await button.isVisible(), `${name} is visible in advanced options`);
        const b = await button.boundingBox();
        assert(b && b.x >= q.x - 1 && b.x + b.width <= q.right + 1, 'advanced action fits queue width');
        if (width <= 700) assert(b.height >= 44, 'mobile advanced action has 44px touch height');
      }
      await page.screenshot({ path: `${stem}.png`, animations: 'disabled' });
      await summary.click();
      await queue.getByRole('button', { name: '收起队列', exact: true }).click();
      assert.equal(await queue.getByRole('button', { name: '展开队列', exact: true }).getAttribute('aria-expanded'), 'false');
      await queue.locator('.composer-queue-list').waitFor({ state: 'hidden' });
      near((await queue.boundingBox()).width, m.composer.width, 'collapsed queue retains composer width');
      await queue.getByRole('button', { name: '展开队列', exact: true }).click();
      assert.equal(await queue.getByRole('button', { name: '收起队列', exact: true }).getAttribute('aria-expanded'), 'true');
      await item.waitFor();
      assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
      pass(`${theme} ${width}px inspector ${inspectorState}${suffix}: source measures share center; inline queue bounded; actions visible; draft preserved`);
    }

    await page.goto(WEB);
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    // Startup may already expand/open the default project and session.
    if (!await page.locator('[data-session-id="dark"] .session-item').isVisible()) {
      await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    }
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.transcript').getByText('正式 Markdown 水平分隔线。', { exact: true }).waitFor();
    fixture.emit({ type: 'ui_dialog_request', requestId: 'width-ask', toolCallId: 'width-ask-tool', ui: { purpose: 'question', kind: 'input', question: '请确认这张 Ask 卡与正文使用共同宽度。' } });
    await page.locator('[data-request-id="width-ask"]').waitFor({ state: 'attached' });
    await page.locator('.composer textarea').fill(DRAFT);
    await queue.waitFor();
    for (const theme of ['light', 'dark']) {
      await page.setViewportSize({ width: 1920, height: 1000 });
      await setInspector(false);
      await page.getByRole('button', { name: '更多操作', exact: true }).click();
      await page.getByRole('menuitem', { name: '外观与主题', exact: true }).click();
      const appearance = page.getByRole('dialog', { name: '外观与主题', exact: true });
      await appearance.getByLabel('明暗模式', { exact: true }).selectOption(theme);
      await appearance.getByRole('button', { name: '保存', exact: true }).click();
      await appearance.waitFor({ state: 'hidden' });
      await page.waitForFunction(mode => document.documentElement.dataset.mode === mode, theme);
      for (const width of [1920, 1440, 1280, 1000, 390, 320]) {
        for (const state of width >= 1280 ? ['open', 'closed'] : ['closed']) await checkWidth(theme, width, state);
      }
      // The timeline supplies an asymmetric left gutter. Also exercise unequal
      // viewport safe insets without replacing the product's width/gutter rules.
      await page.evaluate(() => {
        document.documentElement.style.setProperty('--app-safe-left', '7px');
        document.documentElement.style.setProperty('--app-safe-right', '11px');
      });
      await checkWidth(theme, 320, 'closed', '-safe');
      await page.evaluate(() => {
        document.documentElement.style.removeProperty('--app-safe-left');
        document.documentElement.style.removeProperty('--app-safe-right');
      });
    }

    await page.setViewportSize({ width: 1920, height: 1000 });

    // Inheritance probes follow the unmodified source-cap matrix. Below 1440px
    // the source inherits the root token; wide screens override it locally.
    await page.setViewportSize({ width: 1280, height: 1000 });
    await setInspector(false);
    await page.evaluate(() => document.documentElement.style.setProperty('--conversation-content-max', '900px'));
    await settle();
    near((await layout()).composer.width, 900, 'root token updates composer');
    near((await queue.boundingBox()).width, 900, 'queue inherits the workspace token');
    await page.locator('section.conversation-panel').evaluate(el => el.style.setProperty('--conversation-reading-max', '700px'));
    await settle();
    near((await layout()).message.width, 700, 'local reading token controls message rows');
    near((await queue.boundingBox()).width, 900, 'queue is independent of the narrower reading cap');
    await page.locator('section.conversation-panel').evaluate(el => el.style.removeProperty('--conversation-reading-max'));
    await page.evaluate(() => document.documentElement.style.removeProperty('--conversation-content-max'));
    assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
    // Fixture data only: reload reads an empty snapshot without a clear command.
    fixture.setQueue([]);
    await page.waitForTimeout(250); // Allow local draft persistence before reload.
    await page.reload();
    await page.locator('.transcript').getByText('正式 Markdown 水平分隔线。', { exact: true }).waitFor();
    await queue.waitFor({ state: 'hidden' });
    assert.equal(await queue.count(), 0, 'empty queue leaves no panel or placeholder');
    assert.equal(await page.locator('.composer textarea').inputValue(), DRAFT);
    assert.equal(fixture.requests.filter(r => ['prompt', 'queue_message', 'cancel_queued', 'clear_queue', 'ui_dialog_response', 'resume_pending', 'bash', 'exec', 'set_api_key'].includes(fixtureCommand(r))).length, 0);
    assert.deepEqual(fixture.requests.filter(r => !['GET', 'HEAD', 'OPTIONS'].includes(r.method) && !r.path.endsWith('/open') && !String(r.body.type).startsWith('get_')), [], 'zero fixture mutation/credential writes beyond opening existing sessions');
    assert.deepEqual(blockedRequests, [], 'no attempted writes or calls outside the isolated boundary');
    assert.deepEqual(errors, []);
    pass('fixture-only read operations, preserved draft, zero model/execution/credential writes or JS errors');
  } catch (error) {
    results.push({ failure: error.message, case: activeCase });
    errors.push(error.stack || error.message);
    if (page) await page.screenshot({ path: 'artifacts/conversation-width-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/conversation-width-validation.json', JSON.stringify({ url: WEB, results, geometry, errors, blockedRequests }, null, 2));
    try { if (browser) await browser.close(); } finally { await fixture.close(); }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
