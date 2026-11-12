// Formal WASM session picker: search, observed states, live updates and phone layout.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(15000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await page.route('**/api/v1/sessions', async route => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch(), body = await response.json();
    for (const session of body.sessions) {
      if (session.sessionId === 'architecture') session.status = 'running';
      if (session.sessionId === 'mobile') session.status = 'waiting_for_input';
      if (session.sessionId === 'rpc') { session.status = 'failed'; session.error = 'Fixture failure'; }
    }
    const base = body.sessions[0];
    body.sessions.push(
      { ...base, sessionId: 'done-example', title: '明确完成的会话', status: 'idle', latestSettlement: { outcome: 'completed' } },
      { ...base, sessionId: 'idle-example', title: '尚未开始的会话', status: 'idle', latestSettlement: null },
      { ...base, sessionId: 'unknown-example', title: '未知状态的会话', status: 'unknown', latestSettlement: null },
      ...Array.from({ length: 28 }, (_, i) => ({ ...base, sessionId: `extra-${i}`, title: `很长的中文历史会话名称，用来验证搜索和滚动 ${i}`, status: 'completed' }))
    );
    await route.fulfill({ response, json: body });
  });
  const modal = page.locator('#session-switcher');
  const input = modal.getByLabel('搜索会话或项目', { exact: true });
  const row = id => modal.locator(`.session-switcher-row[data-session-id="${id}"]`);
  const state = async (id, label) => row(id).locator('.switcher-session-state').getByText(label, { exact: true }).waitFor();
  const open = async () => {
    await page.getByRole('button', { name: '切换会话', exact: true }).click();
    await modal.waitFor();
    await input.waitFor();
    assert(await modal.evaluate(el => {
      const active = document.activeElement;
      return !active?.matches('input, textarea, .session-switcher-row');
    }), 'opening the picker must not automatically focus search or a session row');
  };
  const clear = async () => modal.getByRole('button', { name: '清除会话搜索', exact: true }).click();
  const pass = text => console.log('PASS', text);
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.composer textarea:not(:disabled)').waitFor();
    await page.locator('.composer textarea').fill('切换前保留草稿');
    await page.setViewportSize({ width: 390, height: 844 });
    await open();
    await input.click();
    assert(await input.evaluate(el => el === document.activeElement), 'search receives focus only after interaction');
    const searchStyle = await input.evaluate(el => {
      const s = getComputedStyle(el); return { shadow: s.boxShadow, border: s.borderTopWidth, appearance: s.appearance };
    });
    assert.equal(searchStyle.shadow, 'none', 'only the search container owns the focus ring, not a second inner input frame');
    assert.equal(searchStyle.border, '0px');
    assert.equal(searchStyle.appearance, 'none');
    await state('architecture', '运行中'); await state('mobile', '等待回答'); await state('rpc', '异常');
    await state('done-example', '完成'); await state('idle-example', '空闲'); await state('unknown-example', '状态未知');
    await state('dark', '完成');
    pass('switcher lists running, awaiting answer, failure and completed; idle/unknown never become false success');

    for (const [query, id] of [['  架构 RPI-WORK ', 'architecture'], ['等待回答', 'mobile'], ['异常', 'rpc'], ['architecture', 'architecture'], ['d:\\projects\\pi-rust', 'rpc']]) {
      await input.fill(query);
      assert.equal(await modal.locator('.session-switcher-row').count(), 1, query);
      await row(id).waitFor();
    }
    await input.fill('no-result-不存在');
    await modal.getByRole('status').filter({ hasText: '没有匹配的会话' }).waitFor();
    await clear();
    assert.equal(await input.inputValue(), '');
    assert(await input.evaluate(el => el === document.activeElement));
    assert.equal(await modal.locator('.session-switcher-row').count(), 35);
    pass('Chinese/multi-word/project/path/id/status search, empty state and clear preserve search focus');

    await input.fill('dark');
    fixture.emit({ type: 'agent_start' });
    await state('dark', '运行中');
    assert.equal(await input.inputValue(), 'dark');
    assert(await input.evaluate(el => el === document.activeElement), 'live state updates must not remount search');
    fixture.emit({ type: 'agent_settled', outcome: 'completed', pendingResume: false });
    await state('dark', '完成');
    await clear();
    pass('open picker receives observed live state updates without resetting query or focus');

    const initialMode = await page.evaluate(() => document.documentElement.dataset.mode);
    for (const mode of ['light', 'dark']) {
      await page.evaluate(mode => { document.documentElement.dataset.mode = mode; }, mode);
      for (const [width, height] of [[1440, 1000], [390, 844], [320, 740], [320, 420]]) {
        await page.setViewportSize({ width, height });
        await modal.locator('.session-switcher-results').evaluate(el => { el.scrollTop = 0; });
        const before = await input.boundingBox();
        await modal.locator('.session-switcher-results').evaluate(el => { el.scrollTop = el.scrollHeight; });
        const after = await input.boundingBox();
        assert(Math.abs(before.y - after.y) < 1, 'results scroll independently of search');
        assert(await modal.evaluate(el => el.scrollWidth <= el.clientWidth + 1));
        const bounds = await modal.boundingBox();
        assert(bounds.x >= 0 && bounds.y >= 0 && bounds.x + bounds.width <= width + 1 && bounds.y + bounds.height <= height + 1);
        await modal.locator('.session-switcher-results').evaluate(el => { el.scrollTop = 0; });
        const density = await modal.locator('.session-switcher-results').evaluate(el => {
          const bounds = el.getBoundingClientRect();
          const rows = [...el.querySelectorAll('.session-switcher-row')].map(row => row.getBoundingClientRect());
          return {
            rowHeight: rows[0].height,
            spacing: rows[1].top - rows[0].bottom,
            visible: rows.filter(row => row.top >= bounds.top && row.bottom <= bounds.bottom).length,
          };
        });
        assert(density.rowHeight >= 44 && density.rowHeight <= 56, `compact but touch-friendly rows: ${JSON.stringify(density)}`);
        assert(density.spacing <= 1, 'no large gaps between session cards');
        assert(density.visible >= (height < 500 ? 3 : 8), `more sessions fit without scrolling: ${JSON.stringify(density)}`);
        await page.screenshot({ path: `artifacts/session-switcher-${mode}-${width}-${height}.png`, animations: 'disabled' });
      }
    }
    await page.evaluate(mode => { document.documentElement.dataset.mode = mode; }, initialMode);
    await page.setViewportSize({ width: 390, height: 844 });
    pass('compact touch-friendly rows fit at least 8 sessions on full-height desktop/phones; search stays fixed in light/dark and short viewports');

    await input.fill('架构');
    await input.evaluate(el => el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true })));
    assert(await modal.isVisible(), 'IME Enter must not switch sessions');
    await input.press('ArrowDown');
    assert(await row('architecture').evaluate(el => el === document.activeElement));
    await page.keyboard.press('Enter');
    await modal.waitFor({ state: 'hidden' });
    await page.locator('[data-history-id="user-architecture"]').waitFor();
    await open();
    assert.equal(await input.inputValue(), '', 'reopening starts a clean search');
    await input.fill('dark');
    await input.press('Enter');
    await modal.waitFor({ state: 'hidden' });
    await page.locator('[data-history-id="user-dark"]').waitFor();
    assert.equal(await page.locator('.composer textarea').inputValue(), '切换前保留草稿');
    await open();
    await page.keyboard.press('Escape');
    await modal.waitFor({ state: 'hidden' });
    assert(await page.getByRole('button', { name: '切换会话', exact: true }).evaluate(el => el === document.activeElement));
    pass('IME safety, keyboard selection, clean reopen, draft preservation and Escape focus restoration');
    assert.deepEqual(errors, []);
  } catch (error) {
    await page.screenshot({ path: 'artifacts/session-switcher-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
