const assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).first().click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();

    assert.equal(await page.getByRole('button', { name: '全屏查看消息', exact: true }).count(), 0, 'the right-side full-screen control is removed');
    const timeline = page.getByRole('button', { name: '显示时间线', exact: true });
    assert.equal(await timeline.count(), 1, 'the left-side timeline control remains');
    await timeline.click();
    await page.waitForFunction(() => document.querySelector('.timeline-toggle')?.getAttribute('aria-pressed') === 'true');
    assert.equal(await page.locator('.conversation-timeline').count(), 1);
    await page.getByRole('button', { name: '隐藏时间线', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.timeline-toggle')?.getAttribute('aria-pressed') === 'false');
    assert.equal(await page.locator('.conversation-timeline').count(), 0);
    assert.deepEqual(errors, []);
    console.log('PASS left timeline control remains and the right-side full-screen control is removed');
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
