// Production WASM + isolated fixture only; no real node, credentials or model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 } });
  const errors = [], results = [];
  page.setDefaultTimeout(12000);
  page.on('pageerror', e => errors.push(e.message));
  const nav = () => page.getByRole('navigation', { name: '设置分类', exact: true });
  const choose = name => nav().getByRole('button', { name, exact: true }).click();
  const pass = text => { results.push(text); console.log('PASS', text); };
  try {
    await page.addInitScript(() => {
      const write = Storage.prototype.setItem;
      Storage.prototype.setItem = function (key, value) {
        if (key === 'rpi-work-drafts-v1' && window.__rejectDraftStorage) throw new DOMException('Test quota', 'QuotaExceededError');
        return write.call(this, key, value);
      };
    });
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    assert.deepEqual(await nav().getByRole('button').allTextContents(), ['设置总览', '界面与显示', '工作协同', '供应商', '应用更新', '高级设置']);
    await choose('界面与显示');
    await page.getByRole('navigation', { name: '界面与显示分类' }).getByRole('button', { name: '外观与主题', exact: true }).click();
    const appearance = page.locator('section#appearance.appearance-inline');
    await appearance.waitFor();
    assert.equal(await page.getByRole('dialog', { name: '外观与主题', exact: true }).count(), 0);
    const original = await page.evaluate(() => document.documentElement.dataset.theme);
    await appearance.locator('.theme-preset').filter({ hasText: '纸与苔绿' }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'paper');
    await choose('高级设置');
    await page.waitForFunction(theme => document.documentElement.dataset.theme === theme, original);
    await page.getByRole('heading', { name: '高级设置', exact: true }).waitFor();
    assert.match(await page.locator('.settings-page').innerText(), /本设备/);
    pass('six goal-oriented categories; appearance is embedded and leaving rolls back preview; JSON scope remains explicit');

    await choose('设置总览');
    await page.screenshot({ path: 'artifacts/settings-ux-desktop.png', animations: 'disabled' });
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      for (const label of ['工作协同', '供应商', '应用更新', '高级设置', '界面与显示']) {
        const button = nav().getByRole('button', { name: label, exact: true });
        await button.focus(); await page.keyboard.press('Enter');
        await page.waitForFunction(() => {
          const el = document.querySelector('.settings-navigation [aria-current="page"]');
          const rail = el?.closest('.settings-rail');
          if (!el || !rail) return false;
          const a = el.getBoundingClientRect(), b = rail.getBoundingClientRect();
          return a.left >= b.left - 1 && a.right <= b.right + 1;
        });
        assert(await button.evaluate(e => e === document.activeElement), 'selection must retain keyboard focus');
        assert(await button.evaluate(e => e.getBoundingClientRect().height >= 44));
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
      }
    }
    await page.screenshot({ path: 'artifacts/settings-ux-mobile-320.png', animations: 'disabled' });
    pass('320/390px navigation keeps every category visible, keyboard focus stable and touch targets at least 44px');
    await page.setViewportSize({ width: 1280, height: 960 });
    await choose('工作协同');
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').first().click();
    const field = page.locator('.composer-main textarea');
    await page.waitForFunction(() => document.querySelector('.composer-main textarea')?.dataset.imeReady === 'true');
    await page.evaluate(() => { window.__rejectDraftStorage = true; });
    await field.fill('草稿存储失败时仍可编辑');
    const warning = page.locator('.draft-storage-error');
    await warning.waitFor();
    assert.match(await warning.innerText(), /复制备份/);
    assert.match(await page.locator('.connection-status').innerText(), /在线/);
    assert.equal(await field.inputValue(), '草稿存储失败时仍可编辑');
    await page.evaluate(() => { window.__rejectDraftStorage = false; });
    await field.fill('草稿存储已恢复');
    await warning.waitFor({ state: 'hidden' });
    pass('draft persistence failure is local and persistent, connection state is independent, and successful retry clears the warning');
    await page.getByRole('button', { name: '专注阅读', exact: true }).click();
    await page.getByRole('button', { name: '退出专注阅读', exact: true }).waitFor();
    assert.equal(await field.isVisible(), false);
    await page.keyboard.press('Escape');
    await field.waitFor();
    assert.equal(await field.inputValue(), '草稿存储已恢复');
    pass('reading mode temporarily hides the composer; Escape restores the unchanged draft');
    await page.locator('.sidebar-sessions .session-entry.selected').first().getByRole('button', { name: /的会话操作$/ }).click();
    await page.getByRole('menuitem', { name: '详情与配置', exact: true }).click();
    const sessionSettings = page.getByRole('dialog', { name: '会话设置', exact: true });
    await sessionSettings.waitFor();
    assert.deepEqual(await sessionSettings.getByRole('tab').allTextContents(), ['配置', '运行信息', '会话操作']);
    await sessionSettings.getByRole('tabpanel', { name: '配置', exact: true }).waitFor();
    assert.equal(await sessionSettings.getByRole('button', { name: '删除', exact: true }).count(), 0);
    await sessionSettings.getByRole('tab', { name: '运行信息', exact: true }).click();
    const runtime = sessionSettings.getByRole('tabpanel', { name: '运行信息', exact: true });
    await runtime.waitFor();
    assert.equal(await runtime.locator('input,select,textarea,button').count(), 0);
    await sessionSettings.getByRole('tab', { name: '会话操作', exact: true }).click();
    await sessionSettings.getByRole('button', { name: '删除', exact: true }).click();
    await sessionSettings.getByText('确认删除会话？', { exact: true }).waitFor();
    await sessionSettings.getByRole('button', { name: '取消', exact: true }).click();
    assert.equal(await sessionSettings.getByRole('button', { name: '确认删除', exact: true }).count(), 0);
    await page.keyboard.press('Escape');
    await sessionSettings.waitFor({ state: 'hidden' });
    assert.equal(await field.inputValue(), '草稿存储已恢复');
    assert.equal(fixture.requests.filter(r => r.method === 'DELETE').length, 0);
    pass('session settings separates configuration, read-only runtime and actions; cancelling deletion retains the session and draft');
    await page.locator('.sidebar-sessions .session-entry.selected').first().getByRole('button', { name: /的会话操作$/ }).click();
    await page.getByRole('menuitem', { name: '设计引用', exact: true }).click();
    const design = page.getByRole('dialog', { name: '设计引用', exact: true });
    await design.waitFor();
    assert.equal(await design.getByRole('tablist', { name: '会话设置分类', exact: true }).count(), 0);
    assert.equal(await design.getByRole('button', { name: '删除', exact: true }).count(), 0);
    await page.keyboard.press('Escape');
    await design.waitFor({ state: 'hidden' });
    assert.equal(await field.inputValue(), '草稿存储已恢复');
    pass('design references have an independent session-menu entry and dialog, without session actions or lost draft');

    const copyId = async () => {
      await page.locator('.sidebar-sessions .session-entry.selected').first().getByRole('button', { name: /的会话操作$/ }).click();
      await page.getByRole('menuitem', { name: '复制会话标识', exact: true }).click();
    };
    await page.evaluate(() => Object.defineProperty(navigator, 'clipboard', { configurable: true, value: {
      writeText: async () => { throw new Error('fixture clipboard rejection'); }
    } }));
    await copyId();
    const operationError = page.locator('.workspace-operation-error');
    await operationError.waitFor();
    assert.match(await operationError.innerText(), /手动复制/);
    assert.match(await page.locator('.connection-status').innerText(), /在线/);
    await page.evaluate(() => Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async () => {} } }));
    await copyId();
    const notice = page.locator('.workspace-notice');
    await notice.filter({ hasText: '已复制会话标识' }).waitFor();
    await notice.waitFor({ state: 'hidden', timeout: 9000 });
    assert.equal(await operationError.isVisible(), true, 'operation failure persists after a separate success and its timeout');
    await operationError.getByRole('button', { name: '关闭操作错误', exact: true }).click();
    await operationError.waitFor({ state: 'hidden' });
    assert.equal(await field.inputValue(), '草稿存储已恢复');
    pass('ordinary success expires after six seconds, while a separate operation failure persists until explicitly dismissed');
    await page.locator('.sidebar-sessions .session-entry.selected').first().getByRole('button', { name: /的会话操作$/ }).click();
    await page.getByRole('menuitem', { name: '归档会话', exact: true }).click();
    await notice.getByRole('button', { name: '撤销', exact: true }).waitFor();
    await page.waitForTimeout(6500);
    assert.equal(await notice.getByRole('button', { name: '撤销', exact: true }).isVisible(), true, 'undo is not an expiring acknowledgement');
    await notice.getByRole('button', { name: '撤销', exact: true }).click();
    await notice.filter({ hasText: '会话已恢复' }).waitFor();
    assert.equal(await field.inputValue(), '草稿存储已恢复');
    pass('archive undo remains available beyond the ordinary timeout and restores only the isolated fixture session');
    assert.deepEqual(errors, [], 'no application JavaScript errors');
    await page.screenshot({ path: 'artifacts/settings-ux-workspace.png', animations: 'disabled' });
    assert.equal(fixture.requests.filter(r => r.body?.type === 'prompt').length, 0);
    fs.writeFileSync('artifacts/settings-ux-validation.json', JSON.stringify({ results, errors, scope: 'Chromium synthetic fixture; not physical-device acceptance' }, null, 2));
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
