const fs = require('node:fs'), assert = require('node:assert/strict');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1100, height: 950 } });
  page.setDefaultTimeout(6500);
  const results = [], errors = [];
  page.on('pageerror', e => errors.push(e.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  const idle = () => page.waitForFunction(() => document.querySelector('.review-queue')?.dataset.busy === 'false');
  const panel = page.getByRole('region', { name: '待处理消息', exact: true });
  const draft = page.getByRole('textbox', { name: 'Main draft', exact: true });
  const row = id => panel.locator(`[data-entry-id="${id}"]`);
  const commandCount = type => fixture.requests.filter(r => r.body?.type === type).length;
  const connect = async () => {
    const response = page.waitForResponse(r => r.request().method() === 'POST' && r.url().includes('/commands') && r.request().postDataJSON()?.type === 'get_queue');
    await page.getByRole('button', { name: 'Connect fixture', exact: true }).click();
    await response;
    await idle();
  };
  const options = async () => {
    const details = panel.locator('details.queue-options');
    if (!await details.evaluate(e => e.open)) await details.locator('summary').click();
    return details;
  };
  const refresh = async () => {
    const details = await options();
    await details.getByRole('button', { name: '刷新', exact: true }).click();
    await idle();
  };
  const item = (entryId, text, status = 'queued') => ({ entryId, text, status, queue: 'followUp' });
  fs.mkdirSync('artifacts', { recursive: true });
  try {
    await page.goto('http://127.0.0.1:14182/modules/queue/');
    await page.getByRole('button', { name: 'queue', exact: true }).click();
    await page.getByLabel('Fixture node URL').fill(fixture.url);
    await connect();
    assert.equal(await panel.count(), 0);
    assert.equal(await page.locator('textarea').count(), 1);
    assert.equal(await page.locator('dialog,[role="dialog"],.queue-proposal,select').count(), 0);
    assert.equal(await page.getByLabel('队列消息草稿', { exact: true }).count(), 0);
    pass('empty inline queue has no shelf, modal, proposal or second editor');

    fixture.setQueue([
      item('first', '先到的待处理消息'), item('claimed', '已经被领取', 'reserved'),
      item('second', '后到的待处理消息'), item('unknown', '未知状态', null),
      item('done', '已经处理的历史', 'completed'), item('failed', '失败的历史', 'failed'),
    ]);
    await connect();
    await row('first').waitFor();
    assert.deepEqual(await panel.locator('.composer-queue-item').evaluateAll(rows => rows.map(e => e.dataset.entryId)), ['first', 'claimed', 'second', 'unknown']);
    assert.equal(await panel.locator('.composer-queue-count').textContent(), '4');
    assert.equal(await row('first').getByRole('button', { name: '编辑排队消息' }).isEnabled(), true);
    assert.equal(await row('first').getByRole('button', { name: '移除排队消息' }).isEnabled(), true);
    for (const id of ['claimed', 'unknown']) {
      for (const name of ['编辑排队消息', '移除排队消息']) assert(await row(id).getByRole('button', { name }).isDisabled());
    }
    assert.equal(await panel.getByRole('button', { name: '立即发送排队消息' }).count(), 0);
    assert.equal(await panel.getByText('已经处理的历史', { exact: true }).count(), 0);
    const details = panel.locator('details.queue-options');
    assert.equal(await details.evaluate(e => e.open), false);
    await panel.getByRole('button', { name: '收起队列' }).click();
    assert.equal(await panel.locator('.composer-queue-item').count(), 0);
    assert.equal(await panel.locator('.composer-queue-preview').textContent(), '先到的待处理消息');
    assert.equal(await draft.isVisible(), true);
    await panel.getByRole('button', { name: '展开队列' }).click();
    assert.deepEqual(await panel.locator('.composer-queue-item').evaluateAll(rows => rows.map(e => e.dataset.entryId)), ['first', 'claimed', 'second', 'unknown']);
    await (await options()).getByRole('button', { name: '查看历史', exact: true }).click();
    await idle();
    await details.getByLabel('已处理消息').getByText('已经处理的历史', { exact: true }).waitFor();
    assert.equal(await details.getByLabel('已处理消息').locator('button,textarea').count(), 0);
    await details.locator('summary').click();
    assert.equal(await details.getByLabel('已处理消息').isVisible(), false);
    pass('active wire order, local collapse, queued-only actions and optional terminal history');

    fixture.controls.cancelOutcome = 'claimed';
    await draft.fill('现有主草稿');
    await row('first').getByRole('button', { name: '编辑排队消息' }).click();
    await panel.getByRole('alert').filter({ hasText: /claimed/ }).waitFor();
    await idle();
    assert.equal(await draft.inputValue(), '现有主草稿');
    assert.equal(await row('first').count(), 1);
    fixture.controls.cancelOutcome = null;
    fixture.controls.failCancel = true;
    await row('first').getByRole('button', { name: '编辑排队消息' }).click();
    await panel.getByRole('alert').filter({ hasText: /cancellation rejected/ }).waitFor();
    await idle();
    assert.equal(await draft.inputValue(), '现有主草稿');
    fixture.controls.failCancel = false;
    pass('unconfirmed or failed cancellation never edits the main draft');

    fixture.controls.cancelDelayMs = 300;
    const beforeEdit = commandCount('cancel_queued');
    await row('first').getByRole('button', { name: '编辑排队消息' }).click();
    await page.waitForFunction(() => document.querySelector('.review-queue')?.dataset.busy === 'true');
    assert(await row('second').getByRole('button', { name: '移除排队消息' }).isDisabled());
    await draft.fill('等待确认期间的新文字');
    await idle();
    assert.equal(commandCount('cancel_queued'), beforeEdit + 1);
    assert.equal(await row('first').count(), 0);
    assert.equal(await draft.inputValue(), '等待确认期间的新文字\n先到的待处理消息');
    assert.equal(await page.locator('textarea').count(), 1);
    fixture.controls.cancelDelayMs = 0;
    pass('confirmed edit appends to the latest main draft without a queue editor');

    await row('second').getByRole('button', { name: '移除排队消息' }).click();
    await idle();
    assert.equal(await row('second').count(), 0);
    assert.equal(await draft.inputValue(), '等待确认期间的新文字\n先到的待处理消息');
    fixture.setQueue([item('clear-a', '清空一'), item('clear-b', '清空二')]);
    await refresh();
    await (await options()).getByRole('button', { name: '清空待执行', exact: true }).click();
    await idle();
    assert.equal(await panel.count(), 0);
    assert(commandCount('clear_queue') > 0);
    pass('remove and clear read back the queue and do not change the main draft');

    fixture.setQueue([item('late', '迟到的取回')]);
    fixture.setQueue([item('b', '会话 B 的消息')], 'architecture');
    await connect();
    fixture.controls.cancelDelayMs = 300;
    await row('late').getByRole('button', { name: '编辑排队消息' }).click();
    await page.waitForFunction(() => document.querySelector('.review-queue')?.dataset.busy === 'true');
    await page.getByRole('button', { name: 'Session B', exact: true }).click();
    await row('b').waitFor();
    await idle();
    await draft.fill('B 主草稿');
    // Poll original-scope recovery rather than assuming a timer completed.
    await page.getByRole('button', { name: 'Session A', exact: true }).click();
    await idle();
    await page.waitForFunction(() => document.querySelector('[aria-label="Main draft"]')?.value.endsWith('\n迟到的取回'));
    assert.equal(await row('b').count(), 0);
    await page.getByRole('button', { name: 'Session B', exact: true }).click();
    await row('b').waitFor();
    await idle();
    assert.equal(await draft.inputValue(), 'B 主草稿');
    fixture.controls.cancelDelayMs = 0;
    pass('late confirmed edit recovers only the original session main draft');

    const longText = '手机边界长消息'.repeat(80);
    fixture.setQueue(Array.from({ length: 12 }, (_, i) => item(`phone-${i}`, `${i}: ${longText}`)), 'architecture');
    await refresh();
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      await page.screenshot({ path: `artifacts/ui-queue-module-${width}.png` });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      const box = await panel.boundingBox();
      assert(box && box.x >= 0 && box.x + box.width <= width);
      const listBox = await panel.locator('.composer-queue-list').boundingBox();
      assert(listBox && listBox.height <= 240);
      const mobileRow = await panel.locator('.composer-queue-item').first().evaluate(element => {
        const content = element.querySelector('.composer-queue-content').getBoundingClientRect();
        const actions = element.querySelector('.composer-queue-actions').getBoundingClientRect();
        return { contentBottom: content.bottom, actionsTop: actions.top, actionsRight: actions.right, rowRight: element.getBoundingClientRect().right };
      });
      assert(mobileRow.actionsTop >= mobileRow.contentBottom - 1, `${width}: actions occupy a separate row below the message`);
      assert(mobileRow.actionsRight <= mobileRow.rowRight + 1, `${width}: actions stay inside the queue row`);
    }
    assert.equal(commandCount('prompt'), 0);
    assert.equal(commandCount('queue_message'), 0);
    assert.equal(await page.locator('textarea').count(), 1);
    assert.equal(await page.locator('dialog,[role="dialog"]').count(), 0);
    pass('390/320 inline queue stays bounded with one main draft and no enqueue/prompt lifecycle');
    assert.deepEqual(errors, []);
    pass('no application JavaScript errors');
    fs.writeFileSync('artifacts/queue-module-validation.json', JSON.stringify({
      scope: 'pure inline QueuePanel harness; real workspace E2Es own immediate send, busy Enter, persisted drafts and prompt lifecycle', results, errors,
    }, null, 2));
  } catch (e) {
    await page.screenshot({ path: 'artifacts/ui-queue-module-failure.png' });
    fs.writeFileSync('artifacts/queue-module-validation.json', JSON.stringify({ results, errors, fatal: e.message }, null, 2));
    throw e;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
