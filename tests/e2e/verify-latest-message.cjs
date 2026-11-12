// Formal app, real browser input and deterministic local HTTP/SSE. No model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, hasTouch: true });
  page.setDefaultTimeout(8000);
  const results = [], errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const transcript = () => page.locator('#conversation-panel');
  const latest = () => page.getByRole('button', { name: '回到最新消息', exact: true });
  const bottom = () => page.waitForFunction(() => {
    const e = document.querySelector('#conversation-panel');
    return e && e.scrollHeight - e.clientHeight - e.scrollTop < 3;
  });
  const top = async () => {
    await transcript().hover(); await page.mouse.wheel(0, -100000);
    await page.waitForFunction(() => document.querySelector('#conversation-panel').scrollTop < 2);
    await latest().waitFor();
  };
  const shot = name => page.screenshot({ path: `artifacts/latest-message-${name}.png`, animations: 'disabled' });
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14182/app/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('.sidebar-sessions').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await transcript().getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await page.setViewportSize({ width: 390, height: 560 });
    await transcript().hover(); await page.mouse.wheel(0, 100000); await bottom();
    await top();
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.waitForFunction(() => {
      const e = document.querySelector('#conversation-panel');
      return e.scrollHeight <= e.clientHeight + 1;
    });
    await latest().waitFor({ state: 'hidden' });
    pass('a larger viewport with no hidden messages clears Latest instead of offering a no-op jump');

    await page.locator('.composer textarea').fill('滚动修复不能丢失草稿');
    for (let i = 0; i < 16; i++) fixture.emit({ type: 'message_end', message: { entryId: `latest-${i}`, role: 'assistant', content: [{ type: 'text', text: `滚动验证 ${i}\n${'实际消息内容，用于验证点击最新消息。'.repeat(45)}` }] } });
    await transcript().getByText(/滚动验证 15/).waitFor(); await bottom();
    for (const [width, height] of [[1440, 1000], [390, 844], [320, 844]]) {
      await page.setViewportSize({ width, height });
      await bottom(); await top(); await shot(`before-${width}`);
      if (width < 700) await latest().tap(); else await latest().click();
      await bottom(); await latest().waitFor({ state: 'hidden' });
      await shot(`after-${width}`);
      fixture.emit({ type: 'message_end', message: { entryId: `follow-${width}`, role: 'assistant', content: [{ type: 'text', text: `点击后继续跟随 ${width}\n${'后续输出。'.repeat(120)}` }] } });
      await transcript().getByText(new RegExp(`点击后继续跟随 ${width}`)).waitFor(); await bottom();
      await top(); await latest().focus(); await page.keyboard.press('Enter'); await bottom(); await latest().waitFor({ state: 'hidden' });
      assert.equal(await page.locator('.composer textarea').inputValue(), '滚动修复不能丢失草稿');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    }
    pass('mouse/touch and keyboard Latest reach the actual bottom at desktop/390/320, then resume following');
    await page.emulateMedia({ reducedMotion: 'reduce' }); await top();
    const before = await latest().boundingBox();
    await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
    await page.mouse.down();
    try {
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const pressed = await latest().boundingBox();
      assert(Math.abs(pressed.x - before.x) < 1 && Math.abs(pressed.y - before.y) < 1, `Latest moved on pointer-down: ${JSON.stringify({ before, pressed })}`);
      assert(await latest().evaluate(e => {
        const r = e.getBoundingClientRect();
        return e.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
      }), 'the button must still own the release point');
    } finally { await page.mouse.up(); }
    await bottom(); await latest().waitFor({ state: 'hidden' });
    pass('pressed Latest keeps its fixed anchor and receives pointer-up even with reduced motion');
    assert.equal(fixture.requests.filter(r => r.body.type === 'prompt').length, 0);
    assert.deepEqual(errors, []); pass('reduced-motion jump, draft and zero model calls remain safe');
  } catch (error) {
    await shot('failure');
    results.push({ failure: error.message, geometry: await transcript().evaluate(e => ({ top: e.scrollTop, height: e.scrollHeight, visible: e.clientHeight })).catch(() => null), hitTest: await latest().evaluate(e => {
      const r = e.getBoundingClientRect(), style = getComputedStyle(e);
      return { rect: r.toJSON(), pointerEvents: style.pointerEvents, zIndex: style.zIndex, hit: document.elementsFromPoint(r.x + r.width / 2, r.y + r.height / 2).map(e => `${e.tagName}.${e.className}`) };
    }).catch(() => null) });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/latest-message-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close(); await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
