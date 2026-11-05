// Formal Web UI + isolated HTTP/SSE fixture. No model or application commands.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(12000);
  const errors = [], samples = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  try {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    if (!await page.getByRole('navigation', { name: '设置分类' }).isVisible()) {
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
    }
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.getByRole('status').filter({ hasText: '在线' }).first().waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await page.waitForFunction(() => !document.querySelector('.composer-main textarea')?.disabled);
    fixture.emit({ type: 'agent_start', runId: 'loading-motion' });
    await page.locator('.reply-waiting').getByText('正在处理…', { exact: true }).waitFor();

    for (const [motion, duration] of [['reduce', '1.6s'], ['no-preference', '0.8s']]) {
      await page.emulateMedia({ reducedMotion: motion });
      for (const [selector, name] of [
        ['[data-session-id="dark"] .session-activity-spinner', 'session-activity-spin'],
        ['.reply-waiting .activity-spinner', 'activity-spin'],
      ]) {
        const spinner = page.locator(selector);
        await spinner.waitFor();
        const sample = await spinner.evaluate(async element => {
          // Sample the real animation clock and transform across frames; an
          // animation declaration alone does not prove visible progress.
          await new Promise(resolve => requestAnimationFrame(resolve));
          const style = getComputedStyle(element), animation = element.getAnimations()[0];
          const before = { time: animation?.currentTime, transform: style.transform };
          const initial = {
            name: style.animationName, duration: style.animationDuration,
            iteration: style.animationIterationCount, timing: style.animationTimingFunction,
            state: animation?.playState,
          };
          await new Promise(resolve => setTimeout(resolve, 160));
          return { ...initial, before, after: {
            time: animation?.currentTime, transform: getComputedStyle(element).transform,
          }, connected: element.isConnected };
        });
        assert.equal(sample.name, name, `${motion}/${selector}: spinner animation disabled`);
        assert.equal(sample.duration, duration);
        assert.equal(sample.iteration, 'infinite');
        assert.equal(sample.timing, 'linear');
        assert.equal(sample.state, 'running');
        assert(sample.connected);
        assert(sample.after.time > sample.before.time, `${motion}/${selector}: clock did not advance`);
        assert.notEqual(sample.after.transform, sample.before.transform, `${motion}/${selector}: circle did not rotate`);
        samples.push({ motion, selector, ...sample });
      }
      console.log(`PASS ${motion}: current-session and processing indicators visibly rotate at ${duration}`);
    }

    const sidebarSpinner = '[data-session-id="dark"] .session-activity-spinner';
    const replySpinner = '.reply-waiting .activity-spinner';
    const pump = async (text, count = 8, kind = 'text_delta') => {
      for (let i = 0; i < count; i++) {
        fixture.emit({ type: 'message_update', message: { entryId: 'motion-output', role: 'assistant' }, eventType: kind, delta: text });
        await new Promise(resolve => setTimeout(resolve, 200));
      }
    };
    const speeds = async () => page.evaluate(selectors => selectors.map(selector => {
      const element = document.querySelector(selector), animation = element?.getAnimations()[0];
      return { requested: Number(element?.dataset.outputRate), actual: animation?.playbackRate, time: animation?.currentTime };
    }), [sidebarSpinner, replySpinner]);
    const waitRates = async (min, max = 3.2) => page.waitForFunction(({ selectors, min, max }) => {
      const values = selectors.map(selector => document.querySelector(selector)?.getAnimations()[0]?.playbackRate);
      return values.every(value => value >= min && value <= max) && values[0] === values[1];
    }, { selectors: [sidebarSpinner, replySpinner], min, max });

    await pump('ok ');
    await waitRates(1.01, 1.4);
    const slow = await speeds();
    const animation = await page.locator(replySpinner).evaluateHandle(element => element.getAnimations()[0]);
    await pump('fast '.repeat(120));
    await waitRates(2.5);
    const fast = await speeds();
    assert(fast[1].actual > slow[1].actual);
    assert.equal(await page.locator(replySpinner).evaluate((element, original) => element.getAnimations()[0] === original, animation), true, 'speed changes must not recreate/reset the animation');
    assert(fast[1].time > slow[1].time, 'animation clock must keep advancing through rate changes');
    await animation.dispose();
    samples.push({ slow, fast });
    console.log('PASS faster live output increases both spinner rates without restarting the animation');

    await page.emulateMedia({ reducedMotion: 'reduce' });
    await waitRates(1.6, 1.6);
    samples.push({ reducedFast: await speeds() });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await waitRates(2.5);
    await waitRates(1, 1);
    console.log('PASS media preference limits high-output motion; silent output decays to baseline');

    await pump('private reasoning '.repeat(50), 5, 'thinking_delta');
    await waitRates(2);
    fixture.emit({ type: 'tool_execution_start', toolCallId: 'motion-tool', toolName: 'read', args: { path: 'fixture' } });
    await waitRates(1, 1);
    fixture.emit({ type: 'tool_execution_end', toolCallId: 'motion-tool', toolName: 'read', result: { content: [{ type: 'text', text: 'large tool result '.repeat(1000) }] } });
    await new Promise(resolve => setTimeout(resolve, 600));
    await waitRates(1, 1);
    console.log('PASS reasoning contributes to cadence but tool execution/results do not imply model output');

    await pump('live '.repeat(100), 5);
    await waitRates(2);
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.locator('.transcript').getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor();
    fixture.emit({ type: 'agent_start', runId: 'motion-other-session' }, 'architecture');
    await page.locator('.reply-waiting').getByText('正在处理…', { exact: true }).waitFor();
    assert.equal(await page.locator(replySpinner).evaluate(element => element.getAnimations()[0]?.playbackRate), 1, 'another session must not inherit output speed');
    fixture.emit({ type: 'agent_settled', runId: 'motion-other-session', outcome: 'completed' }, 'architecture');
    await page.locator('.reply-waiting').waitFor({ state: 'hidden' });
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    fixture.emit({ type: 'agent_start', runId: 'motion-new-round' });
    await page.locator('.reply-waiting').getByText('正在处理…', { exact: true }).waitFor();
    await waitRates(1, 1);
    console.log('PASS switching sessions and starting a new round cannot inherit another output speed');

    await page.emulateMedia({ reducedMotion: 'reduce' });
    assert.equal(await page.evaluate(() => {
      const probe = document.createElement('span');
      probe.style.animation = 'activity-spin .8s linear infinite';
      document.body.append(probe);
      const name = getComputedStyle(probe).animationName;
      probe.remove();
      return name;
    }), 'none', 'non-progress animations must still respect reduced motion');
    fixture.emit({ type: 'ui_dialog_request', requestId: 'motion-confirm', ui: {
      purpose: 'question', kind: 'selector', question: '是否继续？', options: ['继续', '取消'],
    } });
    await page.locator('.reply-paused').waitFor();
    assert.equal(await page.locator('.reply-waiting .activity-spinner').count(), 0);
    assert.equal(await page.locator('[data-session-id="dark"] .session-activity-spinner').count(), 0);
    fixture.emit({ type: 'ui_dialog_closed', requestId: 'motion-confirm' });
    fixture.emit({ type: 'agent_settled', runId: 'motion-new-round', outcome: 'completed' });
    await page.locator('.reply-waiting').waitFor({ state: 'hidden' });
    assert.equal(await page.locator('[data-session-id="dark"] .session-activity-spinner').count(), 0);
    await page.waitForFunction(() => window.__rpiOutputMotion?.size === 0);
    assert.deepEqual(errors, []);
    assert(!fixture.requests.some(request => ['prompt', 'queue_message', 'abort'].includes(request.body.type)));
    fs.writeFileSync('artifacts/loading-motion-validation.json', JSON.stringify({ passed: true, samples, errors }, null, 2));
    console.log('PASS decorative motion remains disabled; confirmation and settlement do not falsely spin; zero model calls');
  } catch (error) {
    await page.screenshot({ path: 'artifacts/loading-motion-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
