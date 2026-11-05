// Queue actions use the normal composer lifecycle; there is no proposal editor.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(test, message = 'expected queue operation did not arrive') {
  for (let i = 0; i < 180; i++) { if (await test()) return; await pause(25); }
  throw new Error(message);
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const node = await startFixture(0);
  const seed = [
    { entryId: 'send-first', queue: 'followUp', status: 'queued', text: '立即发送的第一条消息' },
    { entryId: 'send-second', queue: 'steer', status: 'queued', text: '立即发送失败后恢复的第二条消息' },
  ];
  node.setQueue(seed);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(10000);
  const errors = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  const panel = () => page.locator('.message-queue.composer-queue-panel');
  const row = id => panel().locator(`.composer-queue-item[data-entry-id="${id}"]`);
  const draft = () => page.getByLabel('发送给 agent 的消息', { exact: true });
  const commands = type => node.requests.filter(request => request.body?.type === type);
  const reloadQueue = async () => { await page.setViewportSize({ width: 1440, height: 1000 }); await page.reload(); await draft().waitFor(); };

  try {
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'light', density: 'standard' }));
    }, node.url);
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.goto(UI);
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await draft().waitFor();
    assert.equal(await row('send-first').getByRole('button', { name: '立即发送排队消息', exact: true }).isEnabled(), true);
    assert.equal(await page.locator('.composer-main textarea[aria-label="发送给 agent 的消息"]').count(), 1);
    assert.equal(await page.locator('.composer-main textarea').count(), 1);
    assert.equal(await page.getByText(/本地提案|提案编辑/, { exact: false }).count(), 0);

    // Immediate send is cancel-confirmed and then invokes exactly one ordinary prompt.
    await draft().fill('保留在主编辑器的草稿');
    const attachment = page.getByLabel('选择本地附件，可多选', { exact: true });
    await attachment.setInputFiles({ name: 'keep.md', mimeType: 'text/markdown', buffer: Buffer.from('# keep') });
    await page.getByRole('button', { name: '移除 keep.md', exact: true }).waitFor();
    const promptBefore = commands('prompt').length;
    const queueBefore = commands('queue_message').length;
    await row('send-first').getByRole('button', { name: '立即发送排队消息', exact: true }).click();
    await until(() => commands('cancel_queued').some(request => request.body.entryId === 'send-first'));
    await until(() => commands('prompt').length === promptBefore + 1);
    const prompt = commands('prompt').at(-1);
    assert.equal(prompt.body.message, '立即发送的第一条消息');
    assert.equal(commands('queue_message').length, queueBefore, 'immediate send does not queue a second message');
    assert(commands('cancel_queued').at(-1).body.entryId === 'send-first');
    assert(commands('cancel_queued').at(-1).path === prompt.path, 'prompt stays in the selected session');
    assert.equal(await draft().inputValue(), '保留在主编辑器的草稿');
    assert.equal(await page.getByRole('button', { name: '移除 keep.md', exact: true }).count(), 1, 'main attachments survive immediate send');
    await row('send-first').waitFor({ state: 'hidden' });
    pass('immediate send cancels first, prompts once with only that row, and preserves the main draft and attachment');

    // A prompt failure recovers only the explicitly sent row into the scoped draft.
    await reloadQueue();
    node.controls.failPrompt = true;
    await draft().fill('draft retained on retry');
    const failedPromptBefore = commands('prompt').length;
    await row('send-second').getByRole('button', { name: '立即发送排队消息', exact: true }).click();
    await until(() => commands('cancel_queued').filter(request => request.body.entryId === 'send-second').length === 1);
    await until(() => commands('prompt').length === failedPromptBefore + 1);
    await panel().getByRole('alert').waitFor();
    const recovered = await draft().inputValue();
    assert(recovered.includes('draft retained on retry'));
    assert(recovered.includes('立即发送失败后恢复的第二条消息'));
    assert.equal(commands('queue_message').length, queueBefore);
    node.controls.failPrompt = false;
    pass('failed immediate prompt recovers only that row into the session draft and does not create queue_message or another send');

    // A negative cancellation response must prevent the prompt entirely.
    node.setQueue([{ entryId: 'not-found-send', queue: 'followUp', status: 'queued', text: '取消未确认不能发送' }]);
    node.controls.cancelOutcome = 'not_found';
    await reloadQueue();
    const blockedPromptBefore = commands('prompt').length;
    await row('not-found-send').getByRole('button', { name: '立即发送排队消息', exact: true }).click();
    await panel().getByRole('alert').waitFor();
    await pause(100);
    assert.equal(commands('prompt').length, blockedPromptBefore);
    assert.equal(await row('not-found-send').count(), 1);
    node.controls.cancelOutcome = null;
    pass('immediate send does not prompt after a not_found cancellation outcome');

    // A newly started run can overtake the cancellation round trip.
    node.setQueue([{ entryId: 'runtime-race', queue: 'followUp', status: 'queued', text: '状态变化后应取回的消息' }]);
    await reloadQueue();
    await draft().fill('运行状态变化时的新草稿');
    const racePromptBefore = commands('prompt').length;
    node.controls.cancelDelayMs = 350;
    await row('runtime-race').getByRole('button', { name: '立即发送排队消息', exact: true }).click();
    await until(() => commands('cancel_queued').some(request => request.body.entryId === 'runtime-race'));
    node.emit({ type: 'agent_start', runId: 'overtook-withdrawal' });
    await page.getByRole('button', { name: '停止运行', exact: true }).waitFor();
    await until(async () => (await draft().inputValue()).includes('状态变化后应取回的消息'));
    assert((await draft().inputValue()).includes('运行状态变化时的新草稿'));
    assert.equal(commands('prompt').length, racePromptBefore);
    node.controls.cancelDelayMs = 0;
    node.emit({ type: 'agent_settled', runId: 'overtook-withdrawal', outcome: 'completed' });
    pass('runtime changes during withdrawal recover the item into the latest main draft instead of starting another task');

    // Busy Enter has the existing default followUp behavior; modifiers and IME do not send.
    node.setQueue([]);
    await reloadQueue();
    node.emit({ type: 'agent_start', runId: 'busy-queue-contract' });
    await page.getByRole('button', { name: '停止运行', exact: true }).waitFor();
    const busyQueueBefore = commands('queue_message').length;
    const busyPromptBefore = commands('prompt').length;
    await draft().fill('忙碌时回车默认排队');
    await draft().press('Enter');
    await until(() => commands('queue_message').length === busyQueueBefore + 1);
    assert.equal(commands('queue_message').at(-1).body.message, '忙碌时回车默认排队');
    assert.equal(commands('queue_message').at(-1).body.deliverAs, 'followUp');
    assert.equal(commands('prompt').length, busyPromptBefore);
    assert.equal(await page.getByRole('button', { name: '停止运行', exact: true }).isVisible(), true);
    const modifierBefore = commands('queue_message').length;
    await draft().fill('ShiftEnter 不应发送');
    await draft().press('Shift+Enter');
    await pause(100);
    assert.equal(commands('queue_message').length, modifierBefore);
    await draft().evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, bubbles: true, cancelable: true })));
    await pause(100);
    assert.equal(commands('queue_message').length, modifierBefore, 'IME Enter does not enqueue');
    const busyImmediate = panel().getByRole('button', { name: '立即发送排队消息', exact: true });
    assert.equal(await busyImmediate.count(), 0, 'immediate send is unavailable while the agent is running');
    node.emit({ type: 'agent_settled', runId: 'busy-queue-contract', outcome: 'completed' });
    pass('busy Enter queues followUp, ShiftEnter/IME do not send, and the running composer remains a stop control');

    // Once idle, ordinary queue actions become available again and remain bounded on mobile.
    await until(async () => await page.getByRole('button', { name: '发送消息', exact: true }).count() > 0);
    node.setQueue([{ entryId: 'touch-row', queue: 'followUp', status: 'queued', text: '移动端触摸消息' }]);
    await reloadQueue();
    for (const mode of ['light', 'dark']) {
      await page.evaluate(value => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: value, density: 'standard' })), mode);
      await reloadQueue();
      assert.equal(await panel().getByRole('button', { name: '立即发送排队消息', exact: true }).isEnabled(), true);
      for (const width of [390, 320]) {
        await page.setViewportSize({ width, height: 844 });
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
        const box = await panel().boundingBox();
        assert(box && box.x >= 0 && box.x + box.width <= width + 1);
        const buttonBox = await panel().getByRole('button', { name: '立即发送排队消息', exact: true }).boundingBox();
        assert(buttonBox && buttonBox.width >= 44 && buttonBox.height >= 44, `${mode} ${width}: immediate send touch target`);
      }
    }
    pass('idle immediate-send controls remain available with 44px touch targets at 320/390px in light and dark modes');
    assert.deepEqual(errors, []);
    pass('no application JavaScript errors');
    fs.writeFileSync('artifacts/ui-queue-proposals-validation.json', JSON.stringify({ results, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/ui-queue-proposals-failure.png', animations: 'disabled' });
    fs.writeFileSync('artifacts/ui-queue-proposals-validation.json', JSON.stringify({ results, errors, fatal: error.message }, null, 2));
    throw error;
  } finally {
    await browser.close();
    await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
