// Formal Dioxus application; the node is an HTTP fixture, no external model call.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const url = process.env.WORK_UI_URL || 'http://127.0.0.1:4182/app/';

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const results = [], errors = [];
  const pass = text => { results.push(text); console.log('PASS', text); };
  page.on('pageerror', error => errors.push(error.message)); page.setDefaultTimeout(10000);
  const dialog = () => page.getByRole('dialog', { name: '外观与主题', exact: true });
  const open = () => page.getByRole('button', { name: '外观与主题', exact: true }).click();
  const choose = name => dialog().locator('.theme-preset').filter({ hasText: name }).click();
  const mode = value => dialog().getByLabel('明暗模式', { exact: true }).selectOption(value);
  const expectTheme = (theme, mode) => page.waitForFunction(({theme, mode}) => document.documentElement.dataset.theme === theme && document.documentElement.dataset.mode === mode, {theme, mode});
  const capture = path => page.screenshot({ path, animations: 'disabled', timeout: 10000 });
  const computed = (selector, property) => page.locator(selector).first().evaluate((element, property) => getComputedStyle(element).getPropertyValue(property), property);
  const selectSession = async () => {
    const session = page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true });
    if (!await session.isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await session.click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
  };
  const tokens = () => page.evaluate(() => {
    const root = document.documentElement, css = getComputedStyle(root);
    return Object.fromEntries(['bg','side','accent','brand','on-accent','success','danger'].map(key => [key, css.getPropertyValue('--'+key).trim()]));
  });
  try {
    await page.goto(url); await expectTheme('codex', 'light');
    assert.equal((await tokens()).bg, '#ffffff'); assert.equal((await tokens()).accent, '#202020');
    assert.equal((await tokens()).brand, '#202020'); assert.notEqual((await tokens()).danger, (await tokens()).accent);
    pass('fresh formal app defaults to Codex monochrome, retaining semantic status colors');

    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await selectSession();
    assert.equal(await computed('.conversation-user-text', 'font-size'), '15px');
    assert.equal(await computed('.conversation-user-text', 'line-height'), '25.5px');
    assert.equal(await computed('.app-header h1', 'font-size'), '16px');
    assert.equal(await computed('.nav-item', 'font-size'), '14px');
    assert.equal(await computed('.composer', 'border-radius'), '20px');
    assert.equal(await computed('.composer textarea', 'font-size'), '16px');
    assert.equal(await computed('.composer .send-button', 'border-radius'), '999px');
    assert.equal(await computed('.theme-trigger', 'border-top-color'), 'rgba(0, 0, 0, 0)');
    await page.locator('.composer textarea').fill('保留这份草稿，不要因为切换主题丢失。');
    assert.notEqual(await computed('.composer', 'box-shadow'), 'none');
    await page.locator('.app-header h1').click();
    assert.match(await computed('.composer', 'box-shadow'), /1px 4px/);
    await capture('artifacts/codex-theme-light.png');
    pass('Codex type hierarchy, rounded editor, subtle resting shadow and focus feedback affect real elements');

    await open(); assert.equal(await dialog().locator('.theme-preset').count(), 5);
    assert.match(await dialog().locator('.theme-preset').first().innerText(), /Codex/);
    await mode('dark'); await expectTheme('codex', 'dark');
    assert.equal((await tokens()).bg, '#212121'); assert.equal((await tokens()).accent, '#f0f0f0');
    await page.keyboard.press('Escape'); await dialog().waitFor({ state: 'hidden' }); await expectTheme('codex', 'light');
    assert.equal(await page.locator('.composer textarea').inputValue(), '保留这份草稿，不要因为切换主题丢失。');
    pass('Codex is first of five presets; cancelling preview restores theme and preserves composer draft');

    await open(); await mode('dark'); await expectTheme('codex', 'dark');
    await dialog().getByRole('button', { name: '保存', exact: true }).click(); await dialog().waitFor({ state: 'hidden' });
    await capture('artifacts/codex-theme-dark.png');
    await page.reload(); await expectTheme('codex', 'dark');
    await selectSession();
    assert.equal((await tokens()).bg, '#212121');
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('rpi-work-appearance-v1')).theme), 'codex');
    pass('Codex dark palette saves and restores across a formal app reload');

    await open(); await mode('system'); await page.emulateMedia({ colorScheme: 'light' }); await expectTheme('codex', 'light');
    assert.equal((await tokens()).bg, '#ffffff');
    await page.emulateMedia({ colorScheme: 'dark' }); await expectTheme('codex', 'dark'); assert.equal((await tokens()).bg, '#212121');
    await choose('Reddit 橙'); await expectTheme('reddit', 'dark'); assert.equal((await tokens()).brand, '#ff4500');
    assert.equal(await computed('.conversation-user-text', 'font-size'), '13px');
    assert.equal(await computed('.composer', 'border-radius'), '12px');
    assert.equal(await computed('.composer textarea', 'font-size'), '11px');
    await choose('Codex 黑白'); await expectTheme('codex', 'dark'); assert.equal((await tokens()).brand, '#f0f0f0');
    pass('system light/dark changes and switching between existing Reddit and Codex palettes are isolated');

    await mode('light'); await expectTheme('codex', 'light');
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      assert(await dialog().evaluate(element => element.scrollWidth <= element.clientWidth + 1));
      await capture(`artifacts/codex-theme-picker-${width}.png`);
    }
    await dialog().getByRole('button', { name: '保存', exact: true }).click(); await dialog().waitFor({ state: 'hidden' });
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      assert(parseFloat(await computed('.composer textarea', 'font-size')) >= 16);
      const send = await page.locator('.composer .send-button').boundingBox();
      assert(send.width >= 44 && send.height >= 44);
      const titleBox = await page.locator('.quick-session strong').boundingBox();
      const actionsBox = await page.locator('.top-actions').boundingBox();
      assert(titleBox.width >= 24, 'mobile title must remain visible');
      assert(titleBox.x + titleBox.width <= actionsBox.x + 1, 'mobile title overlaps status actions');
      const promptButton = page.getByRole('button', { name: '打开会话提示词配置', exact: true });
      console.log(`CHECK mobile resource action at ${width}px`);
      await promptButton.click();
      const resources = page.getByRole('dialog', { name: '会话资源配置', exact: true });
      await resources.waitFor(); await page.keyboard.press('Escape'); await resources.waitFor({ state: 'hidden' });
      assert((await promptButton.boundingBox()).width >= 44);
      assert(parseFloat(await computed('.session-capability-trigger', 'font-size')) >= 12);
      await page.locator('.section-tabs').evaluate(element => { element.scrollLeft = 0; });
      await page.mouse.move(10, 400);
      await capture(`artifacts/codex-theme-mobile-${width}.png`);
    }
    await capture('artifacts/codex-theme-mobile.png');
    pass('390/320px populated workspace has no title/status overlap; readable resource actions scroll and open correctly');
    for (const [event, phase] of [
      [{type:'agent_start'}, 'running'],
      [{type:'agent_settled',outcome:'failed',pendingResume:false}, 'failed'],
    ]) {
      fixture.emit(event);
      await page.locator(`.top-actions .status-badge.${phase}`).waitFor();
      assert.equal(await page.locator(`.status-badge.${phase}`).evaluate(element => getComputedStyle(element).clipPath), 'none');
      assert((await page.locator('.quick-session strong').boundingBox()).width >= 24);
    }
    fixture.emit({type:'agent_settled',outcome:'completed',pendingResume:false});
    await page.locator('.status-badge.completed').waitFor();
    pass('smallest header still displays active/error states and a usable session title');

    const legacy = await browser.newPage();
    await legacy.addInitScript(() => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme:'reddit', mode:'dark', density:'compact' })));
    await legacy.goto(url); await legacy.waitForFunction(() => document.documentElement.dataset.theme === 'reddit' && document.documentElement.dataset.mode === 'dark');
    assert.equal(await legacy.evaluate(() => document.documentElement.dataset.density), 'compact'); await legacy.close();
    pass('existing saved Reddit/dark/compact choice is not silently replaced');

    const fallback = await browser.newPage();
    await fallback.setContent(`<style>${fs.readFileSync('apps/work-ui/src/shared/theme/codex.css','utf8')}</style>`);
    assert.equal(await fallback.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()), '#ffffff');
    await fallback.evaluate(() => { document.documentElement.dataset.theme='codex'; document.documentElement.dataset.mode='system'; });
    await fallback.emulateMedia({ colorScheme: 'dark' });
    assert.equal(await fallback.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()), '#212121'); await fallback.close();
    pass('generated fallback is styled before appearance hooks and supports CSS-only system dark');
    assert.deepEqual(errors, []);
    fs.writeFileSync('artifacts/codex-theme-validation.json', JSON.stringify({ results, errors, scope: 'formal Dioxus app with HTTP node fixture; no external model' }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/codex-theme-failure.png', animations: 'disabled', timeout: 10000 }); throw error;
  } finally { await browser.close(); await fixture.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
