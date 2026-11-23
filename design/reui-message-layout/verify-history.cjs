const assert = require('node:assert/strict');
const path = require('node:path');

async function checkHistoryViews(browser, baseUrl, checkViewport) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const button = name => page.getByRole('button', { name, exact: true });
  const draft = page.getByRole('textbox', { name: '消息', exact: true });
  const phase = () => page.locator('.conversation').getAttribute('data-history');
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  try {
    await page.clock.install();
    await page.goto(baseUrl + '?history=loading&panel=changes');
    assert.equal(await phase(), 'loading');
    assert.equal(await page.locator('#transcript').getAttribute('aria-busy'), 'true');
    assert.equal(await page.getByRole('heading', { name: '正在加载会话记录', exact: true }).isVisible(), true);
    await draft.fill('加载过程中编辑的草稿');
    assert.equal(await button('发送').isDisabled(), true);
    await draft.press('Enter');
    await page.locator('.composer').evaluate(form => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    assert.equal(await draft.inputValue(), '加载过程中编辑的草稿');
    assert.equal(await page.locator('.demo-turn').count(), 0);
    await checkViewport(page);
    await shot('history-loading');
    await page.clock.runFor(800);
    assert.equal(await phase(), 'ready');
    assert.equal(await page.locator('#transcript').getAttribute('aria-busy'), 'false');
    assert.equal(await page.getByRole('heading', { name: '三种状态已补齐' }).isVisible(), true);
    assert.equal(await button('发送').isEnabled(), true);
    assert.equal(await draft.inputValue(), '加载过程中编辑的草稿');

    await page.goto(baseUrl + '?history=error&panel=changes');
    await draft.fill('失败重试也不能丢失');
    await page.locator('#attach').setInputFiles({ name: 'draft-notes.txt', mimeType: 'text/plain', buffer: Buffer.from('local fixture') });
    await page.clock.runFor(800);
    assert.equal(await phase(), 'error');
    assert.equal(await page.getByRole('alert').filter({ hasText: '未能加载会话记录' }).isVisible(), true);
    assert.equal(await button('发送').isDisabled(), true);
    assert.equal(await page.locator('#attachments').textContent(), 'draft-notes.txt');
    assert.equal(await page.locator('#review-content').textContent(), '加载会话记录后显示变更。');
    await page.locator('.composer').evaluate(form => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    assert.equal(await draft.inputValue(), '失败重试也不能丢失');
    await shot('history-error');
    await button('重试加载').click();
    assert.equal(await phase(), 'loading');
    assert.equal(await page.getByRole('button', { name: '重试加载', exact: true }).count(), 0);
    await page.clock.runFor(800);
    assert.equal(await phase(), 'ready');
    assert.equal(await button('发送').isEnabled(), true);
    assert.equal(await draft.inputValue(), '失败重试也不能丢失');
    assert.equal(await page.locator('#attachments').textContent(), 'draft-notes.txt');
    assert.equal(await draft.evaluate(node => document.activeElement === node), true);
    assert.equal(await page.locator('.review-file').count(), 3);
    await shot('history-retry-ready');

    // A delayed result must not replace another thread, nor an unsent new chat.
    await page.locator('[data-session="connection"]').click();
    await page.clock.runFor(200);
    await page.locator('[data-session="runtime"]').click();
    await page.clock.runFor(600);
    assert.equal(await phase(), 'loading');
    assert.equal(await page.locator('#current-title').textContent(), '梳理运行时入口');
    await page.clock.runFor(200);
    assert.equal(await phase(), 'error');
    await button('重试加载').click();
    await page.clock.runFor(200);
    await page.getByRole('button', { name: /^新对话/ }).click();
    await draft.fill('新对话草稿不受旧请求影响');
    await page.clock.runFor(1200);
    assert.equal(await phase(), 'ready');
    assert.equal(await page.locator('.welcome').isVisible(), true);
    assert.equal(await draft.inputValue(), '新对话草稿不受旧请求影响');
    await draft.press('Enter');
    assert.equal(await page.locator('.welcome').count(), 0);
    assert.equal(await phase(), 'ready');
    assert.equal(await page.locator('.demo-turn').count(), 1);
    await page.clock.runFor(1800);
    assert.match(await page.locator('.demo-turn').textContent(), /本地演示已完成/);
    await checkViewport(page);
    await shot('new-chat-first-turn');
    await page.locator('[data-session="layout"]').click();
    assert.equal(await phase(), 'ready');
    assert.equal(await draft.inputValue(), '失败重试也不能丢失');

    await page.reload();
    await page.locator('[data-session="layout"]').click();
    assert.equal(await phase(), 'loading');
    assert.equal(await draft.inputValue(), '失败重试也不能丢失');
    await page.clock.runFor(800);
    assert.equal(await phase(), 'error');
    assert.equal(await page.locator('#attachments').textContent(), 'draft-notes.txt');
    for (const width of [1440, 1280, 1024]) {
      await page.setViewportSize({ width, height: 940 });
      await checkViewport(page);
    }
    await page.goto(baseUrl + '?view=new&history=loading');
    await page.clock.runFor(1200);
    assert.equal(await phase(), 'ready');
    assert.equal(await page.locator('.welcome').isVisible(), true);
    assert.deepEqual(errors, []);
    console.log('History loading/error/retry, blocked submit, persistent drafts/attachments, stale-result cancellation and first-turn transition passed.');
  } finally {
    await page.close();
  }
}

module.exports = { checkHistoryViews };
