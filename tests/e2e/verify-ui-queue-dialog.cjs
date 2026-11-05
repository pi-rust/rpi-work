// Inline queue acceptance: no legacy dialog or local-proposal surface.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(test, message = 'fixture operation did not arrive') {
  for (let i = 0; i < 180; i++) { if (await test()) return; await pause(25); }
  throw new Error(message);
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const node = await startFixture(0);
  const seed = [
    { entryId: 'queued-first', queue: 'followUp', status: 'queued', text: '队列中的第一条消息' },
    { entryId: 'pending-second', queue: 'steer', status: 'pending', text: '队列中的第二条消息' },
    { entryId: 'claimed-third', queue: 'followUp', status: 'claimed', text: '已预留的第三条消息' },
    { entryId: 'unknown-fourth', queue: 'followUp', status: 'unknown', text: '状态未知的第四条消息' },
    { entryId: 'completed-hidden', queue: 'followUp', status: 'completed', text: '完成后不应显示' },
    { entryId: 'cancelled-hidden', queue: 'followUp', status: 'cancelled', text: '取消后不应显示' },
  ];
  node.setQueue(seed);
  node.setQueue([{ entryId: 'architecture-queued', queue: 'followUp', status: 'queued', text: '架构会话中的消息' }], 'architecture');
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(10000);
  const errors = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  const panel = () => page.locator('.message-queue.composer-queue-panel');
  const row = entryId => panel().locator(`.composer-queue-item[data-entry-id="${entryId}"]`);
  const draft = () => page.getByLabel('发送给 agent 的消息', { exact: true });
  const commands = type => node.requests.filter(request => request.body?.type === type);
  const openSession = async (title, historyText) => {
    await page.locator('.sidebar-sessions').getByRole('button', { name: title, exact: true }).click();
    await page.locator('.transcript').getByText(historyText, { exact: true }).waitFor();
  };
  const reloadQueue = async () => {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.reload();
    await draft().waitFor();
  };

  try {
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard' }));
    }, node.url);
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(UI);
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await draft().waitFor();

    assert.equal(await page.locator('.composer-main .message-queue.composer-queue-panel').count(), 1, 'queue is inline in the composer column');
    assert.equal(await panel().getAttribute('role'), 'region');
    assert.equal(await panel().getAttribute('aria-label'), '待处理消息');
    assert.equal(await page.locator('[role="dialog"]').filter({ has: panel() }).count(), 0, 'queue is not a dialog');
    assert.equal(await page.locator('.composer-main textarea[aria-label="发送给 agent 的消息"]').count(), 1, 'there is one main composer textarea');
    assert.deepEqual(await panel().locator('.composer-queue-item .queue-item-text').allTextContents(), seed.slice(0, 4).map(item => item.text));
    assert.equal(await panel().getByText('完成后不应显示', { exact: true }).count(), 0);
    assert.equal(await panel().getByText('取消后不应显示', { exact: true }).count(), 0);
    assert.equal(await row('claimed-third').getByRole('button', { name: '编辑排队消息', exact: true }).isDisabled(), true);
    assert.equal(await row('claimed-third').getByRole('button', { name: '移除排队消息', exact: true }).isDisabled(), true);
    assert.equal(await row('unknown-fourth').getByRole('button', { name: '编辑排队消息', exact: true }).isDisabled(), true);
    assert.equal(await row('queued-first').getByRole('button', { name: '立即发送排队消息', exact: true }).isEnabled(), true);
    assert.equal(await panel().getByRole('button', { name: '收起队列', exact: true }).getAttribute('aria-expanded'), 'true');
    await panel().getByRole('button', { name: '收起队列', exact: true }).click();
    assert.equal(await panel().getByRole('button', { name: '展开队列', exact: true }).getAttribute('aria-expanded'), 'false');
    await panel().getByRole('button', { name: '展开队列', exact: true }).click();
    const options = panel().locator('details.queue-options');
    assert.equal(await options.getByText('队列选项', { exact: true }).count(), 1);
    await options.locator('summary').click();
    for (const name of ['刷新', '清空待执行', '查看历史']) assert.equal(await options.getByRole('button', { name, exact: true }).count(), 1);
    await options.locator('summary').click();
    assert.equal(await panel().locator('textarea').count(), 0, 'queue has no second textarea or local proposal editor');
    assert.equal(await page.getByText(/本地提案|提案编辑/, { exact: false }).count(), 0, 'local proposals were removed');
    pass('inline queue preserves server order, visibility, collapse state, capabilities, and the advanced options');

    // Removal is the same cancellation command as edit, but never changes the composer.
    await draft().fill('移除操作不应改动的主草稿');
    await row('queued-first').getByRole('button', { name: '移除排队消息', exact: true }).click();
    await until(() => commands('cancel_queued').some(request => request.body.entryId === 'queued-first'));
    await row('queued-first').waitFor({ state: 'hidden' });
    assert.equal(await draft().inputValue(), '移除操作不应改动的主草稿');
    assert.equal(commands('prompt').length, 0);
    pass('removal cancels only the selected queued entry and never sends or overwrites the main draft');

    // Successful edit cancels first, then appends into the same main composer.
    await row('pending-second').getByRole('button', { name: '编辑排队消息', exact: true }).click();
    await until(() => commands('cancel_queued').some(request => request.body.entryId === 'pending-second'));
    await until(async () => (await draft().inputValue()).includes('队列中的第二条消息'));
    const editedDraft = await draft().inputValue();
    assert(editedDraft.includes('移除操作不应改动的主草稿'));
    assert.equal(commands('prompt').length, 0);
    assert.equal(commands('queue_message').length, 0);
    await row('pending-second').waitFor({ state: 'hidden' });
    pass('edit uses confirmed cancellation and appends the entry into the existing main composer without auto-send');

    // A failed or negative cancellation must leave both the row and draft alone.
    node.setQueue([{ entryId: 'cancel-failure', queue: 'followUp', status: 'queued', text: '取消失败仍保留的消息' }]);
    node.controls.failCancel = true;
    await reloadQueue();
    await draft().fill('');
    await row('cancel-failure').getByRole('button', { name: '编辑排队消息', exact: true }).click();
    await panel().getByRole('alert').waitFor();
    assert.equal(await draft().inputValue(), '');
    assert.equal(await row('cancel-failure').count(), 1);
    assert.equal(commands('prompt').length, 0);
    node.controls.failCancel = false;
    node.controls.cancelOutcome = 'not_found';
    await row('cancel-failure').getByRole('button', { name: '编辑排队消息', exact: true }).click();
    await panel().getByRole('alert').waitFor();
    assert.equal(await draft().inputValue(), '');
    assert.equal(await row('cancel-failure').count(), 1);
    assert.equal(commands('prompt').length, 0);
    node.controls.cancelOutcome = null;
    pass('cancellation failures and not_found outcomes expose an in-queue alert without touching draft or creating a second send');

    // The cancellation response may arrive after newer typing.
    node.setQueue([{ entryId: 'late-edit', queue: 'followUp', status: 'queued', text: '延迟取消后放回的旧消息' }]);
    await reloadQueue();
    node.controls.cancelDelayMs = 350;
    await row('late-edit').getByRole('button', { name: '编辑排队消息', exact: true }).click();
    await draft().fill('取消等待期间输入的新草稿');
    await until(() => commands('cancel_queued').some(request => request.body.entryId === 'late-edit'));
    await until(async () => (await draft().inputValue()).includes('延迟取消后放回的旧消息'));
    assert((await draft().inputValue()).includes('取消等待期间输入的新草稿'));
    node.controls.cancelDelayMs = 0;
    pass('late successful edit preserves newer typing while appending the cancelled entry');

    // A response for the old session must not write into the newly selected session.
    node.setQueue([{ entryId: 'switch-edit', queue: 'followUp', status: 'queued', text: '切换会话中的旧消息' }]);
    await reloadQueue();
    node.controls.cancelDelayMs = 350;
    await row('switch-edit').getByRole('button', { name: '编辑排队消息', exact: true }).click();
    await openSession('梳理 Agent 架构', '这是 梳理 Agent 架构 的独立历史。');
    await draft().fill('架构会话自己的草稿');
    await pause(500);
    assert.equal(await draft().inputValue(), '架构会话自己的草稿');
    await openSession('为工作台添加深色模式', '主题切换已经准备好，接下来运行组件测试。');
    await until(async () => (await draft().inputValue()).includes('切换会话中的旧消息'));
    node.controls.cancelDelayMs = 0;
    pass('switching sessions during cancellation scopes the late edit to its originating session');

    // Advanced actions operate on the real queue without introducing another editor.
    node.setQueue([
      { entryId: 'advanced-pending', queue: 'followUp', status: 'queued', text: '高级操作中的待执行消息' },
      { entryId: 'advanced-completed', queue: 'followUp', status: 'completed', text: '历史中的已处理消息' },
    ]);
    await reloadQueue();
    await draft().fill('清空队列时保留的主草稿');
    await panel().locator('details.queue-options summary').click();
    await panel().getByRole('button', { name: '查看历史', exact: true }).click();
    await panel().locator('.queue-history').getByText('历史中的已处理消息', { exact: false }).waitFor();
    assert.equal(await panel().locator('.composer-queue-item').count(), 1);
    await panel().getByRole('button', { name: '清空待执行', exact: true }).click();
    await until(async () => await panel().locator('.composer-queue-item').count() === 0);
    assert.equal(await draft().inputValue(), '清空队列时保留的主草稿');
    assert.equal(commands('clear_queue').length, 1);
    assert.equal(commands('prompt').length, 0);
    assert.equal(commands('queue_message').length, 0);
    pass('advanced history reads terminal entries separately; clear uses the server command without touching the main draft');

    // Rich rows must not be silently downgraded to their text caption.
    node.setQueue([{ entryId: 'rich-message', queue: 'followUp', status: 'queued', text: '图片消息的文字说明', message: { role: 'user', content: [{ type: 'text', text: '图片消息的文字说明' }, { type: 'image', data: 'fixture-only' }] } }]);
    await reloadQueue();
    assert(await row('rich-message').getByRole('button', { name: '编辑排队消息', exact: true }).isDisabled());
    assert.equal(await row('rich-message').getByRole('button', { name: '立即发送排队消息', exact: true }).count(), 0);
    assert(await row('rich-message').getByRole('button', { name: '移除排队消息', exact: true }).isEnabled());
    pass('rich messages cannot lose images through text-only edit or immediate send');

    // Empty queues do not leave an empty shelf/panel behind.
    node.setQueue([]);
    await reloadQueue();
    await until(async () => await page.locator('.message-queue.composer-queue-panel').count() === 0);
    assert.equal(await page.locator('.message-queue.composer-queue-panel').count(), 0);
    pass('empty queue is not rendered');

    // Check the same contract at both light and dark modes and at the two narrow widths.
    node.setQueue([{ entryId: 'geometry', queue: 'followUp', status: 'queued', text: '窄屏几何测试消息' }]);
    for (const mode of ['light', 'dark']) {
      await page.evaluate(value => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: value, density: 'standard' })), mode);
      await reloadQueue();
      for (const width of [390, 320]) {
        await page.setViewportSize({ width, height: 844 });
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
        const box = await panel().boundingBox();
        assert(box && box.x >= 0 && box.x + box.width <= width + 1, `${mode} ${width}: queue stays in viewport`);
        assert(await panel().evaluate(element => element.scrollWidth <= element.clientWidth + 1));
        for (const button of await panel().locator('.composer-queue-item button').all()) {
          const buttonBox = await button.boundingBox();
          assert(buttonBox && buttonBox.width >= 44 && buttonBox.height >= 44, `${mode} ${width}: queue action has a 44px touch target`);
        }
      }
      await page.screenshot({ path: `artifacts/ui-queue-inline-${mode}.png`, animations: 'disabled' });
    }
    pass('inline queue remains bounded and touchable at 320/390px in both light and dark modes');
    assert.deepEqual(errors, []);
    pass('no application JavaScript errors');
    fs.writeFileSync('artifacts/ui-queue-dialog-validation.json', JSON.stringify({ results, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/ui-queue-dialog-failure.png', animations: 'disabled' });
    fs.writeFileSync('artifacts/ui-queue-dialog-validation.json', JSON.stringify({ results, errors, fatal: error.message }, null, 2));
    throw error;
  } finally {
    await browser.close();
    await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
