const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

// Standalone fixture-only check. Does not send prompts or modify product/fixture files.
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], errors = [];
  let node, browser, page;
  const pass = name => { results.push(name); console.log('PASS', name); };
  try {
    node = await startFixture();
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => errors.push(error.message));
    await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    const api = async (path, data) => {
      const response = await context.request.post(new URL(`api/v1/${path}`, node.url).href, { data });
      assert(response.ok(), `${path}: HTTP ${response.status()}`);
      return response.json();
    };
    const extras = [];
    for (let index = 0; index < 24; index++) {
      const { sessionId } = await api('sessions', { projectId: 'rpi-work' });
      extras.push(sessionId);
      await api(`sessions/${sessionId}/rename`, {
        title: `侧栏测试 ${String(index + 1).padStart(2, '0')}：很长的会话名称用于验证窄屏省略与操作按钮互不遮挡`,
      });
    }
    await api('sessions/architecture/metadata', { pinned: true });
    // Archive the fixture's first session to catch naive project-open selection.
    await api('sessions/dark/metadata', { archived: true });
    // Deterministic ordering, not relative-time labels or fixture wall-clock values.
    await context.route(url => url.origin === new URL(node.url).origin && url.pathname === '/api/v1/sessions', async route => {
      if (route.request().method() !== 'GET') return route.continue();
      const response = await route.fetch();
      const payload = await response.json();
      payload.sessions = payload.sessions.map(session => ({
        ...session,
        updatedAt: 1700000000000 + (extras.indexOf(session.sessionId) + 1) * 1000,
      }));
      await route.fulfill({ response, json: payload });
    });
    const project = id => page.locator(`.project-entry[data-project-id="${id}"]`);
    const session = id => page.locator(`.session-entry[data-session-id="${id}"]`);
    const projects = () => page.locator('.sidebar-projects');
    const more = () => project('rpi-work').locator('.project-session-more');
    const menu = () => page.getByRole('menu', { name: '会话操作', exact: true });
    const expandedKey = 'rpi-work-expanded-projects-v1';
    const activeCount = extras.length + 2; // mobile + pinned architecture; dark is archived.
    const defaultIds = ['architecture', ...extras.slice(-4).reverse()];
    const ensureNavigation = async () => {
      const opener = page.getByRole('button', { name: '打开项目导航', exact: true });
      if (await opener.isVisible() && !(await page.locator('.sidebar').evaluate(el => el.classList.contains('open')))) {
        await opener.click();
      }
      await projects().waitFor();
      // The drawer animates in; visibility alone does not mean geometry is settled.
      await page.waitForFunction(() => document.querySelector('.sidebar').getBoundingClientRect().left >= -0.5);
    };
    const setExpanded = async (id, value) => {
      const toggle = project(id).locator('.project-toggle');
      if ((await toggle.getAttribute('aria-expanded')) !== String(value)) await toggle.click();
      await page.waitForFunction(({ id, value }) => document.querySelector(`.project-entry[data-project-id="${id}"] .project-toggle`)?.getAttribute('aria-expanded') === String(value), { id, value });
    };
    const waitIds = async ids => {
      await page.waitForFunction(expected => {
        const actual = [...document.querySelectorAll('.project-entry[data-project-id="rpi-work"] .session-entry')].map(el => el.dataset.sessionId);
        return JSON.stringify(actual) === JSON.stringify(expected);
      }, ids);
    };
    const waitStored = async () => {
      await page.waitForFunction(key => localStorage.getItem(key) !== null, expandedKey);
      return page.evaluate(key => localStorage.getItem(key), expandedKey);
    };
    const checkMore = async remaining => {
      assert.match((await more().innerText()).trim(), new RegExp(`^显示更多[（(]${remaining}[）)]$`));
    };
    const assertClickable = async locator => {
      await locator.scrollIntoViewIfNeeded();
      await locator.click({ trial: true });
      assert(await locator.evaluate(el => {
        const box = el.getBoundingClientRect();
        const hit = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
        return hit === el || el.contains(hit);
      }), 'control is obscured at its center');
    };

    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:4180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await ensureNavigation();
    await setExpanded('rpi-work', true);
    await session(extras.at(-1)).locator('.session-item').click();
    await waitIds(defaultIds);
    await checkMore(activeCount - 5);
    assert.equal(await session('dark').count(), 0);
    await page.screenshot({ path: 'artifacts/sidebar-projects-overview.png', animations: 'disabled' });
    pass('default list contains five recent sessions, prioritizes an old pinned session, and excludes archived sessions');

    await more().click();
    await page.waitForFunction(count => document.querySelectorAll('.project-entry[data-project-id="rpi-work"] .session-entry').length === count, activeCount);
    assert.equal((await more().innerText()).trim(), '收起会话');
    await session(extras[0]).locator('.session-item').click();
    await more().click();
    await waitIds([...defaultIds, extras[0]]);
    assert(await session(extras[0]).locator('.session-item').evaluate(el => el.classList.contains('current')));
    await checkMore(activeCount - 6);
    await more().click();
    await page.waitForFunction(count => document.querySelectorAll('.project-entry[data-project-id="rpi-work"] .session-entry').length === count, activeCount);
    assert.equal(await session(extras[0]).count(), 1);
    pass('show more/collapse report the hidden count and retain an older selected session exactly once beyond the five-session limit');

    // pi-rust is not the selected project: startup auto-expansion cannot mask a persistence bug.
    await setExpanded('pi-rust', false);
    const collapsed = await waitStored();
    await setExpanded('pi-rust', true);
    await page.waitForFunction(({ key, previous }) => localStorage.getItem(key) !== previous, { key: expandedKey, previous: collapsed });
    await page.reload();
    await project('pi-rust').waitFor();
    assert.equal(await project('pi-rust').locator('.project-toggle').getAttribute('aria-expanded'), 'true');
    const expandedState = await waitStored();
    await setExpanded('pi-rust', false);
    await page.waitForFunction(({ key, previous }) => localStorage.getItem(key) !== previous, { key: expandedKey, previous: expandedState });
    await page.reload();
    await project('pi-rust').waitFor();
    assert.equal(await project('pi-rust').locator('.project-toggle').getAttribute('aria-expanded'), 'false');
    pass('pi-rust expansion and collapse survive reload using rpi-work-expanded-projects-v1');

    await ensureNavigation();
    await project('rpi-work').locator('.project-row').click();
    await setExpanded('rpi-work', true);
    assert.equal(await session('dark').count(), 0);
    assert(!(await page.locator('.transcript').getAttribute('data-session-scope'))?.endsWith('::session::dark'));
    assert.equal(node.requests.filter(r => r.path === '/api/v1/sessions/dark/metadata' && r.body.archived === false).length, 0);
    pass('opening a project never restores its archived conversation');

    for (const width of [1440, 390, 320]) {
      await page.setViewportSize({ width, height: width === 1440 ? 1000 : 844 });
      await ensureNavigation();
      await setExpanded('rpi-work', true);
      if ((await more().innerText()).trim() !== '收起会话') await more().click();
      await page.waitForFunction(count => document.querySelectorAll('.project-entry[data-project-id="rpi-work"] .session-entry').length === count, activeCount);
      const top = page.locator('.sidebar .new-task');
      const footer = page.getByRole('button', { name: '打开设置', exact: true });
      await projects().evaluate(el => { el.scrollTop = 0; });
      const before = { top: await top.boundingBox(), footer: await footer.boundingBox() };
      assert(before.top && before.footer, `fixed controls missing at ${width}`);
      const geometry = await projects().evaluate(el => {
        const box = el.getBoundingClientRect(), style = getComputedStyle(el);
        return { top: box.top, bottom: box.bottom, left: box.left, right: box.right, height: el.clientHeight, content: el.scrollHeight, overflow: style.overflowY };
      });
      assert(['auto', 'scroll'].includes(geometry.overflow), JSON.stringify(geometry));
      assert(geometry.content > geometry.height, `test list must overflow at ${width}`);
      assert(before.top.y + before.top.height <= geometry.top + 2, 'New task must be above the project scroller');
      assert(before.footer.y >= geometry.bottom - 2, 'settings must be below the project scroller');
      assert(geometry.left >= -1 && geometry.right <= width + 1, 'project scroller escapes viewport');
      const sidebarScroll = await page.locator('.sidebar').evaluate(el => el.scrollTop);
      await projects().hover();
      await page.mouse.wheel(0, 10000);
      await page.waitForFunction(() => document.querySelector('.sidebar-projects').scrollTop > 0);
      await projects().evaluate(el => { el.scrollTop = el.scrollHeight; });
      const after = { top: await top.boundingBox(), footer: await footer.boundingBox() };
      assert(Math.abs(before.top.y - after.top.y) <= 1, 'New task moved when projects scrolled');
      assert(Math.abs(before.footer.y - after.footer.y) <= 1, 'settings moved when projects scrolled');
      assert.equal(await page.locator('.sidebar').evaluate(el => el.scrollTop), sidebarScroll);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'page has horizontal overflow');
      assert(before.footer.y + before.footer.height <= (width === 1440 ? 1000 : 844) + 1);
      await assertClickable(footer);
      await assertClickable(top);
      // Every list session is idle: no green/status dot should be painted for it.
      assert.equal(await project('rpi-work').locator('.session-dot').evaluateAll(dots => dots.filter(dot => {
        const style = getComputedStyle(dot), box = dot.getBoundingClientRect();
        return style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity) > 0 && box.width > 0 && box.height > 0;
      }).length), 0, 'idle sessions show status dots');
      const entry = session(extras.at(-1));
      await entry.scrollIntoViewIfNeeded();
      await entry.hover();
      const actions = entry.locator('.session-row-menu');
      await assertClickable(actions);
      await actions.click();
      await menu().waitFor();
      await page.keyboard.press('Escape');
      await menu().waitFor({ state: 'hidden' });
      await menu().waitFor({ state: 'hidden' });
      // Reach the row action via actual keyboard navigation, including focus-within styling.
      await entry.locator('.session-item').focus();
      await page.waitForFunction(id => document.activeElement?.closest('.session-entry')?.dataset.sessionId === id, extras.at(-1));
      await page.keyboard.press('Tab');
      assert(await actions.evaluate(el => el === document.activeElement), `session menu is not keyboard reachable: ${await page.evaluate(() => document.activeElement?.outerHTML)}`);
      assert(await actions.evaluate(el => Number(getComputedStyle(el).opacity) > 0), 'focused menu is transparent');
      await page.keyboard.press('Enter');
      await menu().waitFor();
      await page.keyboard.press('Escape');
      const plus = project('pi-rust').locator('.project-new-session');
      await plus.scrollIntoViewIfNeeded();
      await plus.focus();
      await assertClickable(plus);
      assert(await plus.evaluate(el => Number(getComputedStyle(el).opacity) > 0));
      if (width !== 1440) {
        const box = await plus.boundingBox();
        assert(Math.round(box.width) >= 44 && Math.round(box.height) >= 44, JSON.stringify(box));
      }
      await page.screenshot({ path: `artifacts/sidebar-projects-${width}.png`, animations: 'disabled' });
      pass(`${width}px: independent project scrolling, fixed top/footer, idle status, hover/focus menus and project + remain accessible`);
    }

    const createsBefore = node.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/sessions').length;
    await project('pi-rust').locator('.project-new-session').click();
    await page.waitForFunction(() => document.querySelector('.app-header h1')?.textContent === '新会话');
    const creates = node.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/sessions');
    assert.equal(creates.length, createsBefore + 1);
    assert.equal(creates.at(-1).body.projectId, 'pi-rust');
    assert.equal(await page.locator('.sidebar').evaluate(el => el.classList.contains('open')), false);
    pass('project + really creates in that project and closes the mobile sidebar');
    assert.equal(node.requests.filter(r => r.body.type === 'prompt').length, 0);
    assert.equal(node.requests.filter(r => r.method === 'DELETE').length, 0);
    assert.deepEqual(errors, []);
    pass('no prompts, destructive requests, or application JavaScript errors');
  } catch (error) {
    errors.push(error.stack || error.message);
    if (page) await page.screenshot({ path: 'artifacts/sidebar-projects-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/sidebar-projects-validation.json', JSON.stringify({ results, errors }, null, 2));
    if (browser) await browser.close();
    if (node) await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
