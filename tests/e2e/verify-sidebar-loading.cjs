// Formal UI + local HTTP/SSE fixture only. No build and no external model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
async function until(predicate) {
  for (let i = 0; i < 200; i++) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw new Error('Expected fixture request did not arrive');
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], errors = [], geometry = [], screenshots = [];
  let browser, page;
  const pass = name => { results.push(name); console.log('PASS', name); };
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    for (const theme of ['light', 'dark']) for (const width of [1280, 390, 320]) {
      const fixture = await startFixture();
      const context = await browser.newContext({ viewport: { width: 1280, height: 950 } });
      let releasePrompt, releaseAbort;
      try {
        await context.addInitScript(mode => localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode, density: 'comfortable' })), theme);
        await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
        // Hold only this fixture prompt, so slow CI cannot skip the sending state.
        let holdPrompt = true;
        const promptGate = new Promise(resolve => { releasePrompt = resolve; });
        await context.route('**/api/v1/sessions/dark/commands', async route => {
          if (route.request().method() === 'POST' && route.request().postDataJSON()?.type === 'prompt' && holdPrompt) await promptGate;
          await route.continue();
        });
        page = await context.newPage();
        page.setDefaultTimeout(10000);
        page.on('pageerror', error => errors.push(`${theme}/${width}: ${error.message}`));
        const row = id => page.locator(`.session-entry[data-session-id="${id}"]`);
        const composer = page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
        const button = page.locator('.composer-actions .send-button');
        const ensureNavigation = async () => {
          const opener = page.getByRole('button', { name: '打开项目导航', exact: true });
          if (await opener.isVisible() && !(await page.locator('.sidebar').evaluate(el => el.classList.contains('open')))) await opener.click();
          await page.waitForFunction(() => document.querySelector('.sidebar')?.getBoundingClientRect().left >= -0.5);
        };
        const closeNavigation = async () => {
          if (await page.getByRole('button', { name: '关闭项目导航', exact: true }).isVisible()) {
            await page.keyboard.press('Escape');
            await page.getByRole('button', { name: '关闭项目导航', exact: true }).waitFor({ state: 'hidden' });
          }
        };
        const select = async id => { await ensureNavigation(); await row(id).locator('.session-item').click(); };
        const activity = async (id, label, spinning) => {
          await page.waitForFunction(({ id, label, spinning }) => {
            const entry = document.querySelector(`.session-entry[data-session-id="${id}"]`);
            return entry && (entry.querySelector('.session-activity-label')?.textContent.trim() || '') === label
              && entry.querySelectorAll('.session-activity-spinner').length === Number(spinning)
              && entry.querySelectorAll('.session-updated').length === Number(!spinning)
              && (entry.querySelector('.session-item')?.getAttribute('aria-description') || '') === label;
          }, { id, label, spinning });
          assert.equal(await row(id).locator('.activity-spinner').count(), 0, 'sidebar must use its own spinner class');
          assert.equal(await row(id).locator('.session-updated').count(), spinning ? 0 : 1,
            'active loading/sending/running/cancelling hides relative time; idle/confirmation/failure restores it');
          if (spinning) assert(await row(id).evaluate(entry => {
            const title = entry.querySelector('.session-title').getBoundingClientRect();
            const spinner = entry.querySelector('.session-activity-spinner').getBoundingClientRect();
            return spinner.left >= title.right - 1;
          }), 'session loading belongs on the right of its title');
        };
        const reducedProgress = async () => {
          for (const locator of [row('dark').locator('.session-activity-spinner'), button.locator('.activity-spinner')]) {
            assert.equal(await locator.count(), 1);
            const motion = await locator.evaluate(el => {
              const css = getComputedStyle(el);
              return { name: css.animationName, duration: css.animationDuration, state: el.getAnimations()[0]?.playState };
            });
            assert.notEqual(motion.name, 'none', 'essential progress must remain animated');
            assert.equal(motion.duration, '1.6s');
            assert.equal(motion.state, 'running');
          }
        };
        let readySize;
        const measure = async (state, iconSelector) => {
          await page.waitForFunction(() => {
            const el = document.querySelector('.composer-actions .send-button');
            return el && getComputedStyle(el).backgroundColor === 'rgba(0, 0, 0, 0)';
          });
          const value = await button.evaluate((el, selector) => {
            const b = el.getBoundingClientRect(), icon = el.querySelector(selector), i = icon?.getBoundingClientRect();
            const css = getComputedStyle(el), frame = getComputedStyle(el, '::before');
            const context = document.createElement('canvas').getContext('2d');
            const normalize = color => { context.fillStyle = color; return context.fillStyle; };
            return { x: b.x, y: b.y, width: b.width, height: b.height, iconWidth: i?.width, iconHeight: i?.height,
              // A rotating square's axis-aligned bounds expand mid-animation;
              // verify its intrinsic CSS size without freezing normal motion.
              iconCssWidth: icon ? parseFloat(getComputedStyle(icon).width) : null,
              iconCssHeight: icon ? parseFloat(getComputedStyle(icon).height) : null,
              visualWidth: parseFloat(frame.width), visualHeight: parseFloat(frame.height), radius: frame.borderRadius,
              background: css.backgroundColor, frameBackground: frame.backgroundColor,
              accent: normalize(css.getPropertyValue('--accent').trim()), painted: normalize(frame.backgroundColor),
              dx: i ? Math.abs(i.x + i.width / 2 - b.x - b.width / 2) : null,
              dy: i ? Math.abs(i.y + i.height / 2 - b.y - b.height / 2) : null };
          }, iconSelector);
          assert(value.iconWidth > 0 && value.iconHeight > 0, `${state}: missing icon`);
          assert(value.dx < 1 && value.dy < 1, `${theme}/${width}/${state}: off-center ${JSON.stringify(value)}`);
          assert(value.x >= 0 && value.x + value.width <= width + 1, `${state}: outside viewport`);
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${state}: horizontal overflow`);
          assert.equal(value.visualWidth, 28, `${state}: compact disk width`);
          assert.equal(value.visualHeight, 28, `${state}: compact disk height`);
          assert.equal(value.radius, '50%', `${state}: circular disk`);
          assert.equal(value.background, 'rgba(0, 0, 0, 0)', `${state}: transparent hit target`);
          assert.equal(value.painted, value.accent, `${state}: accent disk fill`);
          const expectedIconSize = iconSelector === '.icon' ? 12 : iconSelector === '.activity-spinner' ? 14 : 8;
          assert.equal(value.iconCssWidth, expectedIconSize);
          assert.equal(value.iconCssHeight, expectedIconSize);
          if (width <= 390) {
            assert(value.width >= 44 && value.height >= 44, `${state}: undersized mobile target`);
          }
          if (!readySize) readySize = value;
          assert(Math.abs(value.width - readySize.width) < 1 && Math.abs(value.height - readySize.height) < 1, `${state}: button dimensions changed`);
          geometry.push({ theme, viewportWidth: width, state, ...value });
          // Capture real states already driven below; never manufacture CSS classes.
          if (width <= 390) {
            const shot = `artifacts/sidebar-loading-${theme}-${width}-${state}.png`;
            await page.locator('.composer-main').screenshot({ path: shot, animations: 'disabled' });
            screenshots.push(shot);
          }
        };
        const expectStop = async () => {
          await page.getByRole('button', { name: '停止运行', exact: true }).waitFor();
          await page.waitForFunction(() => document.querySelector('.composer-actions .send-button')?.disabled === false);
          assert(await button.isEnabled());
          assert(await button.evaluate(el => el.classList.contains('is-loading')), 'active class remains for compatibility');
          assert.equal(await button.locator('.composer-stop-icon').count(), 1);
          assert.equal(await button.locator('.activity-spinner, .icon').count(), 0, 'stop must be square only');
          // The themed background color transitions when send becomes stop.
          await page.waitForFunction(() => {
            const el = document.querySelector('.composer-actions .send-button.is-stop');
            return el && getComputedStyle(el).backgroundColor === 'rgba(0, 0, 0, 0)';
          });
          const stopVisual = await button.evaluate(el => {
            const circle = getComputedStyle(el, '::before');
            return { width: parseFloat(circle.width), height: parseFloat(circle.height), background: getComputedStyle(el).backgroundColor };
          });
          assert.equal(stopVisual.width, 28, 'stop visual must match the compact send disk, independently of the touch target');
          assert.equal(stopVisual.width, stopVisual.height, 'stop visual must stay circular');
          assert.equal(stopVisual.background, 'rgba(0, 0, 0, 0)', 'the larger hit target must not paint an oversized stop button');
        };
        const expectSpinnerOnly = async () => {
          await button.locator('.activity-spinner').waitFor();
          assert(await button.isDisabled());
          assert.equal(await button.locator('.activity-spinner').count(), 1);
          assert.equal(await button.locator('.composer-stop-icon, .icon').count(), 0);
        };

        await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
        await page.waitForFunction(mode => document.documentElement.dataset.theme === 'codex' && document.documentElement.dataset.mode === mode, theme);
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
        await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
        await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
        await page.getByRole('button', { name: '连接节点', exact: true }).click();
        await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
        await page.getByRole('button', { name: '返回会话', exact: true }).click();
        await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
        await select('dark');
        await composer.waitFor();
        await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
        await page.setViewportSize({ width, height: width === 1280 ? 950 : 844 });
        await activity('dark', '', false);
        assert(await button.isDisabled());
        await measure('ready-empty', '.icon');
        await composer.fill('验证侧栏即时 loading');
        assert(await button.isEnabled());
        await measure('ready', '.icon');
        fixture.controls.promptDelayMs = 100;
        await button.click();
        await page.locator('.pending-user-message').getByText('验证侧栏即时 loading', { exact: true }).waitFor();
        await activity('dark', '发送中', true);
        await expectSpinnerOnly();
        await measure('sending', '.activity-spinner');
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await reducedProgress();
        await page.emulateMedia({ reducedMotion: 'no-preference' });
        await select('architecture');
        await page.locator('.transcript').getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor();
        await activity('architecture', '', false);
        await activity('dark', '发送中', true);
        assert.equal(await page.locator('.composer-actions .is-loading').count(), 0, 'background sending cannot leak into current composer');
        holdPrompt = false; releasePrompt();
        await activity('dark', '等待回复', true);
        await activity('architecture', '', false);
        await select('dark');
        await page.locator('.reply-waiting').getByText('等待回复…', { exact: true }).waitFor();
        await expectStop();
        await measure('accepted-stop', '.composer-stop-icon');
        fixture.emit({ type: 'message_end', message: { entryId: 'sidebar-user', role: 'user', content: '验证侧栏即时 loading' } });
        await page.locator('.pending-user-message').waitFor({ state: 'hidden' });
        await activity('dark', '等待回复', true);
        const runId = `sidebar-${theme}-${width}`;
        fixture.emit({ type: 'agent_start', runId });
        fixture.emit({ type: 'message_update', message: { entryId: 'sidebar-answer', role: 'assistant' }, eventType: 'text_delta', delta: '侧栏测试的流式输出' });
        await page.locator('.transcript').getByText('侧栏测试的流式输出', { exact: true }).waitFor();
        await activity('dark', '运行中', true);
        await expectStop();
        fixture.emit({ type: 'tool_execution_start', toolCallId: 'sidebar-read', toolName: 'read', args: { path: 'README.md' } });
        await page.locator('.tool-call').filter({ hasText: 'README.md' }).first().waitFor();
        await activity('dark', '运行中', true);
        fixture.emit({ type: 'tool_execution_end', toolCallId: 'sidebar-read', toolName: 'read', result: { content: [{ type: 'text', text: 'Sidebar fixture read complete' }] } });
        await expectStop();
        await measure('running-stop', '.composer-stop-icon');
        assert.equal(await page.locator('.activity-spinner').count(), 1, 'only transcript waiting uses the general spinner');
        await composer.fill('运行时保留的新草稿');
        await expectStop();
        assert.equal(await button.locator('.composer-stop-icon').count(), 1);
        await measure('running-with-draft-stop', '.composer-stop-icon');
        await composer.fill('');
        await expectStop();
        await select('architecture');
        await page.locator('.transcript').getByText('这是 梳理 Agent 架构 的独立历史。', { exact: true }).waitFor();
        // Background sessions are NOT subscribed. Only last-observed running state is retained.
        await activity('dark', '运行中', true);
        await activity('architecture', '', false);
        assert.equal(await page.locator('.composer-actions .is-loading').count(), 0);
        await select('dark');
        await activity('dark', '运行中', true);
        await expectStop();
        await measure('restored-stop', '.composer-stop-icon');
        const composerShot = `artifacts/sidebar-loading-${theme}-${width}-composer.png`;
        await page.screenshot({ path: composerShot, animations: 'disabled' }); screenshots.push(composerShot);
        await ensureNavigation();
        const shot = `artifacts/sidebar-loading-${theme}-${width}.png`;
        await page.screenshot({ path: shot, animations: 'disabled' }); screenshots.push(shot);
        await closeNavigation();
        fixture.emit({ type: 'agent_end', runId });
        await activity('dark', '运行中', true);
        fixture.emit({ type: 'agent_settled', runId, outcome: 'completed' });
        await activity('dark', '', false);
        await page.locator('.reply-waiting').waitFor({ state: 'hidden' });
        await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
        assert(await button.isDisabled());
        await measure('settled', '.icon');

        fixture.controls.promptDelayMs = 0;
        fixture.controls.failPrompt = true;
        await composer.fill('节点拒绝时保留这份草稿');
        await button.click();
        await page.locator('.composer-foot [role=status]').filter({ hasText: /指令未.*接受/ }).waitFor();
        await activity('dark', '', false);
        await page.locator('.pending-user-message').waitFor({ state: 'hidden' });
        await page.locator('.reply-waiting').waitFor({ state: 'hidden' });
        assert.equal(await composer.inputValue(), '节点拒绝时保留这份草稿');
        assert(await button.isEnabled());
        await measure('rejected-draft', '.icon');
        fixture.controls.failPrompt = false;
        // Accept abort at the fixture, but withhold its HTTP response until
        // after settlement: a late acknowledgement must not revive loading.
        const abortGate = new Promise(resolve => { releaseAbort = resolve; });
        await page.route('**/api/v1/sessions/dark/commands', async route => {
          if (route.request().postDataJSON()?.type !== 'abort') return route.fallback();
          const response = await route.fetch();
          await abortGate;
          await route.fulfill({ response });
        });
        await composer.fill('验证停止中的双处 loading');
        await button.click();
        await activity('dark', '等待回复', true);
        fixture.emit({ type: 'agent_start', runId: 'sidebar-cancel' });
        await composer.fill('停止前保留的草稿');
        await expectStop();
        await button.click();
        await page.waitForFunction(() => document.querySelector('.composer-actions .send-button')?.getAttribute('aria-label') === '正在停止…');
        await until(() => fixture.requests.some(request => request.path === '/api/v1/sessions/dark/commands' && request.body.type === 'abort'));
        await activity('dark', '停止中', true);
        await expectSpinnerOnly();
        await measure('cancelling', '.activity-spinner');
        await composer.fill('停止期间保留的新草稿');
        await expectSpinnerOnly();
        await measure('cancelling-with-draft', '.activity-spinner');
        assert.equal(await composer.inputValue(), '停止期间保留的新草稿');
        await button.evaluate(element => element.click());
        assert.equal(fixture.requests.filter(request => request.body.type === 'abort').length, 1, 'disabled cancelling control must not repeat abort');
        assert.equal(fixture.requests.filter(request => request.body.type === 'queue_message').length, 0, 'stop with a draft must not enqueue it');
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await reducedProgress();
        // Abort acknowledgement is not settlement: both indicators must still be present.
        assert.equal(await page.locator('.reply-waiting').count(), 1);
        await activity('dark', '停止中', true);
        fixture.emit({ type: 'agent_settled', runId: 'sidebar-cancel', outcome: 'interrupted' });
        await activity('dark', '', false);
        await page.locator('.reply-waiting').waitFor({ state: 'hidden' });
        await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
        assert(await button.isEnabled());
        assert.equal(await composer.inputValue(), '停止期间保留的新草稿');
        await measure('cancel-settled', '.icon');
        const abortAcknowledged = page.waitForResponse(response => response.url().includes('/sessions/dark/commands') && response.request().postDataJSON()?.type === 'abort');
        releaseAbort();
        await (await abortAcknowledged).finished();
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.equal(await page.getByText('停止请求已发送，等待 agent_settled…', { exact: true }).count(), 0, 'late abort acknowledgement must not overwrite settlement');
        await activity('dark', '', false);
        fixture.emit({ type: 'agent_start', runId: 'sidebar-confirm' });
        fixture.emit({ type: 'ui_dialog_request', requestId: 'sidebar-question', ui: { purpose: 'question', kind: 'selector', question: '是否继续？', options: ['继续', '取消'] } });
        await page.locator('[data-request-id="sidebar-question"]').waitFor();
        await activity('dark', '等待确认', false);
        assert.equal(await page.locator('.reply-waiting .activity-spinner').count(), 0, 'waiting for user input is not active work');
        await expectStop();
        await measure('awaiting-input', '.composer-stop-icon');
        fixture.emit({ type: 'ui_dialog_closed', requestId: 'sidebar-question' });
        fixture.emit({ type: 'agent_settled', runId: 'sidebar-confirm', outcome: 'completed' });
        await activity('dark', '', false);
        fixture.emit({ type: 'agent_start', runId: 'sidebar-failed' });
        fixture.emit({ type: 'agent_settled', runId: 'sidebar-failed', outcome: 'failed' });
        await activity('dark', '执行失败', false);
        await page.getByRole('button', { name: '发送消息', exact: true }).waitFor();
        assert.equal(await button.locator('.activity-spinner, .composer-stop-icon').count(), 0);
        assert.equal(fixture.requests.filter(request => request.body.type === 'prompt').length, 3, 'typing/switching must not submit extra prompts');
        pass(`${theme}/${width}: sending/waiting/text/tools/background/settlement/rejection/cancellation/late-ack/confirmation/failure; centered stable controls and reduced motion`);
      } catch (error) {
        if (page && !page.isClosed()) await page.screenshot({ path: `artifacts/sidebar-loading-${theme}-${width}-failure.png`, animations: 'disabled' }).catch(() => {});
        throw error;
      } finally {
        releasePrompt?.();
        releaseAbort?.();
        await context.close();
        await fixture.close();
      }
    }
    assert.deepEqual(errors, []);
    pass('six fixture-only light/dark desktop/mobile cases completed without application JavaScript errors');
  } catch (error) {
    errors.push(error.stack || error.message);
    if (page && !page.isClosed()) await page.screenshot({ path: 'artifacts/sidebar-loading-failure.png', animations: 'disabled' }).catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync('artifacts/sidebar-loading-validation.json', JSON.stringify({ scope: 'fixture only; no model calls', results, errors, geometry, screenshots }, null, 2));
    await browser?.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
