// Real integrated WASM (build/serve separately) + isolated HTTP/WebSocket fixture per case.
// Fixture-only commands: no provider/model calls, credentials, builds or fixture edits.
// node --check validates syntax ONLY; run this file separately for browser evidence.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const UI = process.env.WORK_UI_URL || 'http://127.0.0.1:14180/';
const PLAY = require('../../assets/icons/lucide-catalog.json').catalog.play.nodes[0][1].points;
const DRAFT = '保留这份新草稿 😀，不要重发旧问题';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const results = [], errors = [], geometry = [];
let browser;
async function until(check, label) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) { if (await check()) return; await delay(30); }
  throw new Error(`Timed out: ${label}`);
}
function envelope(body, data, error = null) {
  return { type: 'response', id: body.id ?? null, command: body.type, success: error === null, data, error };
}
async function test(name, configure, verify) {
  const started = Date.now(), commands = [], external = [], pageErrors = [], gates = [];
  const proof = { name, status: 'FAIL', commands, external, errors: pageErrors };
  const policy = { missing: [], offline: false, resumeError: null, resumeGate: null, promptGate: null, configGate: null };
  let fixture, context, page, run = 0, tearingDown = false;
  function gate(label) {
    let resolve, timer;
    const promise = new Promise(done => { resolve = done; });
    const item = { label, entered: false, released: false, delivered: false };
    item.release = () => { if (!item.released) { item.released = true; clearTimeout(timer); resolve(); } };
    item.wait = async () => {
      item.entered = true;
      if (!item.released) timer = setTimeout(() => { pageErrors.push(`Expired gate: ${label}`); item.release(); }, 15000);
      await promise;
    };
    gates.push(item); return item;
  }
  try {
    fixture = await startFixture(0);
    context = await browser.newContext({ viewport: { width: 1280, height: 950 } });
    page = await context.newPage(); page.setDefaultTimeout(10000);
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('websocket', socket => {
      const socketUrl = new URL(socket.url());
      const nodeUrl = new URL(fixture.url);
      if (nodeUrl.hostname === socketUrl.hostname && nodeUrl.port === socketUrl.port && policy.offline) socket.close();
    });
    const h = {
      page, fixture, policy, commands, gate,
      field: () => page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true }),
      button: () => page.locator('.composer-actions .send-button'),
      next: () => page.getByRole('button', { name: '继续会话', exact: true }),
      send: () => page.getByRole('button', { name: '发送消息', exact: true }),
      stop: () => page.getByRole('button', { name: '停止运行', exact: true }),
      session: id => page.locator(`[data-session-id="${id}"] .session-item`),
      count: type => commands.filter(item => item.body.type === type).length,
      actions: () => commands.filter(item => ['prompt', 'resume_pending', 'queue_message'].includes(item.body.type)),
      async ready() {
        await h.field().waitFor();
        await page.waitForFunction(() => {
          const field = document.querySelector('.composer-main textarea');
          return field && !field.disabled && field.dataset.imeReady === 'true';
        });
      },
      async select(id) { await h.session(id).click(); await h.ready(); },
      async idle() {
        // Wait for the actual primary button, not merely the WebSocket write completing.
        await h.send().waitFor(); await until(async () => await h.button().getAttribute('aria-busy') !== 'true', 'settled composer rendered');
      },
      async noContinue() {
        await h.button().waitFor();
        await until(async () => !await h.next().count() && !await page.locator('.composer-actions .is-continue').count(), 'non-Continue primary rendered');
      },
      async continued() {
        await h.next().waitFor();
        await until(async () => !await h.next().isDisabled(), 'Continue enabled');
        assert.match(await h.next().getAttribute('class'), /\bis-continue\b/);
        assert.equal((await h.next().innerText()).trim(), '', 'Continue is icon-only');
        assert.equal(await h.next().locator(`svg[data-icon="play"] polygon[points="${PLAY}"]`).count(), 1, 'Continue uses the play icon, not refresh/send/stop');
        assert.equal(await h.field().inputValue(), '');
        assert.equal(await page.locator('.composer-actions button').count(), 1);
      },
      async fail(pendingResume = false, sessionId = 'dark') {
        const runId = `continue-failure-${++run}`;
        fixture.emit({ type: 'agent_start', runId }, sessionId);
        fixture.emit({ type: 'agent_settled', runId, outcome: 'failed', pendingResume, error: 'Fixture-only failure' }, sessionId);
        if (sessionId === 'dark') {
          await h.continued();
          const failure = h.page.getByRole('alert', { name: '当前会话执行失败', exact: true });
          await failure.waitFor();
          assert.match(await failure.innerText(), /Fixture-only failure/);
          assert(await failure.getByRole('button', { name: '重试当前会话', exact: true }).isEnabled());
        }
        return runId;
      },
      async settle(outcome = 'completed', pendingResume = false) {
        const runId = `continue-settle-${++run}`;
        fixture.emit({ type: 'agent_start', runId });
        fixture.emit({ type: 'agent_settled', runId, outcome, pendingResume });
        if (outcome === 'failed') await h.continued(); else await h.idle();
      },
      async waiting() {
        await h.stop().waitFor(); assert(await h.stop().isEnabled());
        assert.equal(await page.locator('.composer-actions .composer-stop-icon').count(), 1);
        assert.equal(await page.locator('.composer-actions .activity-spinner').count(), 0);
        await page.locator('.reply-waiting').waitFor();
        await h.noContinue();
      },
      async pending() {
        const pending = page.getByRole('button', { name: '正在发送…', exact: true });
        await pending.waitFor(); assert(await pending.isDisabled());
        assert.equal(await pending.getAttribute('aria-busy'), 'true');
        assert.equal(await pending.locator('.activity-spinner').count(), 1);
        assert.equal(await page.locator('.composer-actions .composer-stop-icon').count(), 0);
      },
      async connect() {
        await page.getByRole('button', { name: '打开设置', exact: true }).click();
        await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '工作协同', exact: true }).click();
        if (!await page.getByLabel('节点地址', { exact: true }).isVisible()) await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
        await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
        await page.getByRole('button', { name: '连接节点', exact: true }).click();
        await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
        await page.getByRole('button', { name: '返回会话', exact: true }).click();
        if (!await h.session('dark').isVisible()) await page.locator('.project-row').filter({ hasText: 'rpi-work' }).click();
        await h.select('dark');
      },
      async unchanged(draft = DRAFT) {
        await h.send().waitFor(); assert(await h.send().isEnabled());
        assert.equal(await h.field().inputValue(), draft);
        assert.equal(await page.locator('.reply-waiting').count(), 0);
        assert.equal(await page.locator('.composer-actions .is-loading').count(), 0);
        assert(!await page.locator('.composer-foot [role=status]').innerText().then(text => text.includes('OLD_SCOPE_REJECT')));
      },
    };
    if (configure) await configure(h);
    const origins = [new URL(UI).origin, new URL(fixture.url).origin];
    await context.route('**/*', async route => {
      const request = route.request(), url = new URL(request.url());
      if (!['http:', 'https:'].includes(url.protocol)) return route.continue();
      if (url.origin === 'https://api.github.com') return route.fulfill({ status: 404, json: {}, headers: { 'access-control-allow-origin': '*' } });
      if (!origins.includes(url.origin)) { external.push(request.url()); return route.abort('blockedbyclient'); }
      // Reject API traffic to the UI's own node too: only this test fixture may
      // execute commands. The UI origin is allowed solely for static app assets.
      if (url.origin !== origins[1]) {
        if (url.pathname.startsWith('/api/')) { external.push(request.url()); return route.abort('blockedbyclient'); }
        return route.continue();
      }
      if (policy.offline) return route.abort('connectionfailed');
      if ((url.pathname === '/api/v1/capabilities' || url.pathname.endsWith('/access')) && policy.missing.length) {
        const response = await route.fetch(), data = await response.json();
        data.supportedCommands = data.supportedCommands.filter(command => !policy.missing.includes(command));
        return route.fulfill({ response, json: data });
      }
      const configurationWrite = request.method() === 'POST' && url.pathname.endsWith('/configuration');
      if (request.method() !== 'POST' || (!url.pathname.endsWith('/commands') && !configurationWrite)) return route.continue();
      const wireBody = request.postDataJSON();
      const body = configurationWrite
        ? { type: wireBody.configId === 'model' ? 'set_model' : 'set_thinking_level', ...(wireBody.configId === 'model' ? { modelId: wireBody.value } : { level: wireBody.value }) }
        : wireBody;
      commands.push({ path: url.pathname, body, wireBody });
      const reply = (data, error = null) => route.fulfill({ status: 200,
        headers: { 'access-control-allow-origin': origins[0], 'access-control-allow-credentials': 'true' },
        json: envelope(body, data, error) });
      if (['set_api_key', 'queue_message'].includes(body.type)) {
        pageErrors.push(`Forbidden command: ${body.type}`); return reply(null, 'Forbidden in Continue regression');
      }
      if (body.type === 'resume_pending') {
        const held = policy.resumeGate, error = policy.resumeError;
        policy.resumeGate = null;
        try {
          if (held) await held.wait();
          // Native ACK deliberately emits NO agent_start/agent_settled. Tests
          // must prove it is acceptance only, never manufacture completion.
          await reply({ accepted: error === null }, error);
        } catch (error) { if (!tearingDown) throw error; }
        finally { if (held) held.delivered = true; }
        return;
      }
      const held = body.type === 'prompt' ? policy.promptGate : body.type === 'set_model' ? policy.configGate : null;
      if (held) {
        if (body.type === 'prompt') policy.promptGate = null; else policy.configGate = null;
        // Use the real fixture's promptDelayMs and failPrompt controls. Gate the
        // resulting response, not shared fixture source or product semantics.
        try {
          const response = await route.fetch(); held.snapshot = await response.json();
          await held.wait(); await route.fulfill({ response, json: held.snapshot });
        } catch (error) { if (!tearingDown) throw error; }
        finally { held.delivered = true; }
        return;
      }
      return route.continue();
    });
    await page.goto(UI); await h.connect();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。', { exact: true }).waitFor();
    await h.idle(); await verify(h);
    assert.equal(h.count('set_api_key'), 0); assert.equal(h.count('queue_message'), 0);
    assert.deepEqual(external, []); assert.deepEqual(pageErrors, []);
    proof.status = 'PASS'; console.log('PASS', name);
  } catch (error) {
    proof.error = error.stack || String(error); errors.push({ name, error: proof.error });
    console.error('FAIL', name, error.message);
    if (page) await page.screenshot({ path: `artifacts/session-continue-failure-${results.length + 1}.png`, animations: 'disabled', timeout: 5000 }).catch(() => {});
  } finally {
    tearingDown = true; gates.forEach(item => item.release());
    if (context) await context.close().catch(() => {});
    if (fixture) await fixture.close();
    proof.durationMs = Date.now() - started; results.push(proof);
    fs.writeFileSync('artifacts/session-continue-validation.json', JSON.stringify({ results, errors, geometry,
      scope: 'Separately built real WASM UI; isolated HTTP/WebSocket only, no live model/provider or credential writes.',
      limitations: 'node --check is syntax-only, not browser evidence. Screenshots cover light/dark at 1280/390/320; no physical-device claim.' }, null, 2));
  }
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  try {
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    await test('01 true failed settlement renders icon-only play disk; normal disk and responsive themes', null, async h => {
      await h.noContinue(); assert(await h.send().isDisabled());
      await h.fail(false);
      for (const mode of ['light', 'dark']) {
        await h.page.setViewportSize({ width: 1280, height: 950 });
        await h.page.getByRole('button', { name: '外观与主题', exact: true }).click();
        const theme = h.page.getByRole('dialog', { name: '外观与主题', exact: true });
        await theme.getByLabel('明暗模式', { exact: true }).selectOption(mode);
        await theme.getByRole('button', { name: '保存', exact: true }).click(); await theme.waitFor({ state: 'hidden' });
        for (const width of [1280, 390, 320]) {
          await h.page.setViewportSize({ width, height: width === 1280 ? 950 : 844 });
          await h.continued();
          const sample = await h.next().evaluate(button => {
            const box = element => { const r = element.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
            const icon = button.querySelector('svg'), css = getComputedStyle(icon), frame = getComputedStyle(button, '::before');
            return { button: box(button), icon: box(icon),
              iconStyle: { opacity: css.opacity, visibility: css.visibility, color: css.color, fill: css.fill, stroke: css.stroke },
              frame: [parseFloat(frame.width), parseFloat(frame.height)], radius: frame.borderRadius,
              overflow: document.documentElement.scrollWidth - innerWidth };
          });
          geometry.push({ mode, width, state: 'continue', ...sample });
          assert(sample.overflow <= 1); assert(sample.button.x >= -1 && sample.button.x + sample.button.width <= width + 1);
          assert.deepEqual(sample.frame, [28, 28]); assert.equal(sample.radius, '50%');
          assert.equal(sample.iconStyle.visibility, 'visible'); assert(Number(sample.iconStyle.opacity) > 0);
          assert.equal(sample.iconStyle.fill, sample.iconStyle.color); assert.equal(sample.iconStyle.stroke, 'none');
          assert.notEqual(sample.iconStyle.fill, 'rgba(0, 0, 0, 0)');
          assert.deepEqual([sample.icon.width, sample.icon.height], [12, 12]);
          assert(Math.abs(sample.icon.x + sample.icon.width / 2 - sample.button.x - sample.button.width / 2) <= 1, 'play centered horizontally');
          assert(Math.abs(sample.icon.y + sample.icon.height / 2 - sample.button.y - sample.button.height / 2) <= 1, 'play centered vertically');
          for (const rect of [sample.icon]) {
            assert(rect.width > 0 && rect.height > 0);
            assert(rect.x >= sample.button.x - 1 && rect.x + rect.width <= sample.button.x + sample.button.width + 1);
            assert(rect.y >= sample.button.y - 1 && rect.y + rect.height <= sample.button.y + sample.button.height + 1);
          }
          if (width < 400) assert(sample.button.width >= 44 && sample.button.height >= 44);
          await h.page.screenshot({ path: `artifacts/session-continue-${mode}-${width}.png`, animations: 'disabled' });
          await h.field().fill(DRAFT); await h.send().waitFor(); await h.noContinue();
          const normal = await h.send().evaluate(button => {
            const frame = getComputedStyle(button, '::before'), r = button.getBoundingClientRect();
            return { disk: [parseFloat(frame.width), parseFloat(frame.height)], radius: frame.borderRadius, hit: [r.width, r.height] };
          });
          geometry.push({ mode, width, state: 'normal', ...normal });
          assert.deepEqual(normal.disk, [28, 28]); assert.equal(normal.radius, '50%');
          if (width < 400) assert(normal.hit.every(size => size >= 44));
          await h.page.screenshot({ path: `artifacts/session-continue-normal-${mode}-${width}.png`, animations: 'disabled' });
          await h.field().fill(''); await h.continued();
        }
      }
      assert.equal(h.actions().length, 0);
    });

    await test('02 native click deduplicates pending ACK; ACK/agent_end wait for actual settlement', h => {
      h.policy.resumeGate = h.gate('native resume ACK');
    }, async h => {
      const held = h.policy.resumeGate; await h.fail(true);
      await h.next().evaluate(button => { button.click(); button.click(); });
      await until(() => held.entered, 'native resume captured'); await h.pending();
      await h.field().press('Enter'); await delay(100);
      assert.equal(h.count('resume_pending'), 1); assert.equal(h.count('prompt'), 0);
      const request = h.actions()[0]; assert(request.path.includes('/dark/'));
      assert(!Object.hasOwn(request.body, 'message'), 'native resume sends no prompt text');
      held.release(); await until(() => held.delivered, 'native ACK delivered'); await h.waiting();
      h.fixture.emit({ type: 'agent_start', runId: 'native-resumed' });
      h.fixture.emit({ type: 'agent_end', runId: 'native-resumed' });
      await delay(120); await h.waiting();
      h.fixture.emit({ type: 'agent_settled', runId: 'native-resumed', outcome: 'completed', pendingResume: false });
      await h.idle(); await h.noContinue(); assert(await h.send().isDisabled());
    });

    await test('03 native Enter uses same action; rejected resume keeps Continue and permits one retry', h => {
      h.policy.resumeError = 'Fixture resume rejected'; h.policy.resumeGate = h.gate('rejected native ACK');
    }, async h => {
      const held = h.policy.resumeGate; await h.fail(true); await h.field().press('Enter');
      await until(() => held.entered, 'Enter native resume captured'); await h.pending();
      held.release(); await until(() => held.delivered, 'native rejection delivered'); await h.continued();
      assert.equal(h.count('resume_pending'), 1); assert.equal(h.count('prompt'), 0);
      h.policy.resumeError = null; await h.next().click(); await h.waiting();
      assert.equal(h.count('resume_pending'), 2); assert.equal(h.count('prompt'), 0);
      await h.settle('failed', true); await h.continued();
    });

    await test('04 no explicit pendingResume sends literal Continue once; prompt ACK is not completion', h => {
      h.fixture.controls.promptDelayMs = 150; h.policy.promptGate = h.gate('fallback prompt ACK');
    }, async h => {
      const held = h.policy.promptGate; await h.fail(false);
      await h.next().evaluate(button => { button.click(); button.click(); });
      await until(() => held.entered, 'fallback prompt captured'); await h.pending();
      await h.field().press('Enter'); assert.equal(h.actions().length, 1);
      assert.equal(h.count('resume_pending'), 0); assert.equal(h.actions()[0].body.message, '继续');
      assert.equal(h.actions()[0].body.type, 'prompt');
      // A newer draft typed while the literal fallback is in flight belongs to
      // the user, not the old submission; its ACK must not clear that revision.
      await h.field().fill(DRAFT);
      held.release(); await until(() => held.delivered, 'fallback ACK delivered'); await h.waiting();
      assert.equal(await h.field().inputValue(), DRAFT);
      await h.settle(); await h.noContinue(); assert(await h.send().isEnabled());
      assert.equal(h.actions().length, 1);
    });

    await test('05 pendingResume without resume capability falls back to one literal Enter prompt', h => {
      h.policy.missing = ['resume_pending'];
    }, async h => {
      await h.fail(true); await h.field().press('Enter'); await h.waiting();
      assert.equal(h.count('resume_pending'), 0); assert.equal(h.count('prompt'), 1);
      assert.equal(h.actions()[0].body.message, '继续'); await h.settle();
    });

    await test('06 typed draft sends normally; rejection retains draft, never substitutes previous content', null, async h => {
      await h.fail(true); await h.field().fill(DRAFT); await h.send().waitFor(); await h.noContinue();
      h.fixture.controls.failPrompt = true; await h.send().click();
      await h.page.locator('.composer-foot [role=status]').filter({ hasText: /指令未.*接受/ }).waitFor();
      await h.unchanged(); assert.equal(h.count('resume_pending'), 0);
      assert.equal(h.count('prompt'), 1); assert.equal(h.actions()[0].body.message, DRAFT);
      await h.field().fill(''); await h.continued();
    });

    await test('07 local attachment pending prevents empty-composer Continue and both dispatch paths', null, async h => {
      await h.fail(true);
      await h.page.getByLabel('选择本地附件，可多选', { exact: true }).setInputFiles({ name: 'continue-private.png', mimeType: 'image/png',
        buffer: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/WZ0AAAAASUVORK5CYII=', 'base64') });
      await h.page.getByRole('button', { name: '预览 continue-private.png', exact: true }).waitFor();
      await h.noContinue(); assert(!await h.button().isDisabled()); // Attachment-only send is capability-gated, not Continue.
      await h.field().press('Enter'); await h.button().evaluate(button => button.click()); await delay(150);
      assert.equal(h.actions().length, 0); assert.equal(await h.field().inputValue(), '');
      assert.equal(await h.page.locator('#attachment-send-guard').count(), 0);
    });

    await test('08 completed/interrupted/tool-error-alone/active/cancelling/offline never offer Continue', null, async h => {
      for (const outcome of ['completed', 'interrupted']) { await h.fail(true); await h.settle(outcome); await h.noContinue(); }
      h.fixture.emit({ type: 'agent_start', runId: 'tool-error-only-run' });
      h.fixture.emit({ type: 'tool_execution_start', toolCallId: 'only-tool-error', toolName: 'read', args: { path: 'missing.txt' } });
      h.fixture.emit({ type: 'tool_execution_end', toolCallId: 'only-tool-error', toolName: 'read', isError: true,
        result: { isError: true, content: [{ type: 'text', text: 'TOOL_ERROR_NOT_AGENT_FAILURE' }] } });
      // Output can be collapsed: wait on the actual failed tool status rather
      // than assuming that its body text is visible in the current theme.
      await h.page.locator('.transcript .call-status[title="失败"]').waitFor();
      await h.stop().waitFor(); await h.noContinue();
      h.fixture.emit({ type: 'agent_settled', runId: 'tool-error-only-run', outcome: 'completed' }); await h.idle();
      await h.fail(true); h.fixture.emit({ type: 'agent_start', runId: 'active-not-continue' });
      await h.stop().waitFor(); await h.noContinue(); await h.stop().click();
      const cancelling = h.page.getByRole('button', { name: '正在停止…', exact: true });
      await cancelling.waitFor(); assert(await cancelling.isDisabled()); await h.noContinue();
      assert.equal(h.count('abort'), 1); assert.equal(h.actions().length, 0);
      h.fixture.emit({ type: 'agent_settled', runId: 'active-not-continue', outcome: 'interrupted' }); await h.idle();
      await h.fail(true); h.policy.offline = true; h.fixture.disconnect('dark', true);
      await h.noContinue(); assert(await h.button().isDisabled());
      await h.field().press('Enter'); await delay(100); assert.equal(h.actions().length, 0);
    });

    for (const pendingResume of [false, true]) {
      await test(`09${pendingResume ? 'b' : 'a'} read-only/unsupported node cannot invent a text fallback (pending=${pendingResume})`, h => {
        // With no pending resume, even a resume-capable node cannot substitute
        // that command for a missing prompt capability. Also cover neither.
        h.policy.missing = pendingResume ? ['prompt', 'resume_pending'] : ['prompt'];
      }, async h => {
        // Keep the failed-session affordance visible but truthfully disabled
        // when neither permitted recovery path exists.
        h.fixture.emit({ type: 'agent_start', runId: 'readonly-failure' });
        h.fixture.emit({ type: 'agent_settled', runId: 'readonly-failure', outcome: 'failed', pendingResume });
        // Sidebar is an independent visible witness that settlement was consumed.
        await h.page.locator('[data-session-id="dark"]').filter({ hasText: /失败|待恢复/ }).waitFor();
        await h.next().waitFor(); assert(await h.next().isDisabled());
        assert.match(await h.next().getAttribute('title'), /不支持/);
        await h.field().press('Enter'); await delay(150);
        assert.equal(h.actions().length, 0); assert.equal(await h.field().inputValue(), '');
      });
    }

    for (const sameSession of [false, true]) {
      await test(`10${sameSession ? 'b' : 'a'} late native ${sameSession ? 'old-generation rejection' : 'old-session ACK'} preserves current draft/UI`, h => {
        h.policy.resumeGate = h.gate('old scope native response');
        h.policy.resumeError = sameSession ? 'OLD_SCOPE_REJECT' : null;
      }, async h => {
        const held = h.policy.resumeGate; await h.fail(true); await h.next().click();
        await until(() => held.entered, 'old resume response held'); await h.pending();
        await h.select('architecture'); await h.idle();
        if (sameSession) { await h.select('dark'); await h.continued(); }
        await h.field().fill(DRAFT); await h.unchanged();
        held.release(); await until(() => held.delivered, 'late old response delivered'); await delay(200);
        await h.unchanged(); assert.equal(h.count('resume_pending'), 1); assert.equal(h.count('prompt'), 0);
      });
    }

    await test('11 same-address reconnect fences late old-connection resume rejection', h => {
      h.policy.resumeGate = h.gate('old connection resume response'); h.policy.resumeError = 'OLD_SCOPE_REJECT';
    }, async h => {
      const held = h.policy.resumeGate; await h.fail(true); await h.next().click();
      await until(() => held.entered, 'old connection request captured'); await h.pending();
      const reads = h.count('get_available_models');
      await h.connect(); await until(() => h.count('get_available_models') > reads, 'new connection catalog arrived');
      await h.continued(); await h.field().fill(DRAFT); await h.unchanged();
      held.release(); await until(() => held.delivered, 'late old connection response delivered'); await delay(200);
      await h.unchanged(); assert.equal(h.count('resume_pending'), 1); assert.equal(h.count('prompt'), 0);
    });

    await test('12 pending model configuration disables Continue and Enter until authoritative readback', h => {
      h.policy.configGate = h.gate('model setter ACK');
    }, async h => {
      const held = h.policy.configGate; await h.fail(true);
      await h.page.getByRole('button', { name: '选择供应商与模型', exact: true }).click();
      const picker = h.page.getByRole('dialog', { name: '选择供应商与模型', exact: true }); await picker.waitFor();
      const provider = picker.locator('details.composer-provider-group[data-provider="openai"]');
      if (!await provider.evaluate(element => element.open)) await provider.locator('summary').click();
      await picker.locator('.composer-model-option[data-provider="openai"][data-model-id="openai/gpt-6-astra"]').click();
      await until(() => held.entered, 'model mutation response held');
      await h.page.keyboard.press('Escape'); await picker.waitFor({ state: 'hidden' });
      await h.next().waitFor(); assert(await h.next().isDisabled());
      await h.field().press('Enter'); await h.next().evaluate(button => button.click()); await delay(100);
      assert.equal(h.actions().length, 0);
      held.release(); await until(() => held.delivered, 'model mutation ACK released');
      await h.continued(); assert.equal(h.count('set_model'), 1); assert.equal(h.actions().length, 0);
    });
    await test('13 native settlement arriving before ACK cannot resurrect loading or consume a new draft', h => {
      h.policy.resumeGate = h.gate('ACK after settlement');
    }, async h => {
      const held = h.policy.resumeGate; await h.fail(true); await h.next().click();
      await until(() => held.entered, 'native response held'); await h.pending();
      h.fixture.emit({ type: 'agent_start', runId: 'early-settlement' });
      h.fixture.emit({ type: 'agent_settled', runId: 'early-settlement', outcome: 'completed', pendingResume: false });
      await h.idle(); await h.field().fill(DRAFT); await h.unchanged();
      held.release(); await until(() => held.delivered, 'late ACK delivered'); await delay(150);
      await h.unchanged(); assert.equal(h.count('resume_pending'), 1); assert.equal(h.count('prompt'), 0);
    });

    await test('15 empty Enter after fallback ACK cannot send a second Continue before runtime events', null, async h => {
      await h.fail(false); await h.next().click(); await h.waiting();
      assert.equal(h.count('prompt'), 1); assert.equal(await h.field().inputValue(), '');
      for (let i = 0; i < 3; i++) await h.field().press('Enter');
      await delay(150); await h.waiting();
      assert.equal(h.count('prompt'), 1); assert.equal(h.actions().length, 1);
      await h.settle();
    });

    for (const oldError of [null, 'OLD_ATTEMPT_REJECT']) {
      await test(`16 native old ${oldError ? 'rejection' : 'ACK'} cannot consume a newer same-context resume attempt`, h => {
        h.policy.resumeGate = h.gate('old attempt ACK'); h.policy.resumeError = oldError;
      }, async h => {
        const old = h.policy.resumeGate; await h.fail(true); await h.next().click();
        await until(() => old.entered, 'first native request held'); await h.pending();
        h.fixture.emit({ type: 'agent_start', runId: 'first-early-failure' });
        h.fixture.emit({ type: 'agent_settled', runId: 'first-early-failure', outcome: 'failed', pendingResume: true });
        await h.continued();
        const fresh = h.gate('newer attempt ACK'); h.policy.resumeGate = fresh; h.policy.resumeError = null;
        await h.next().click(); await until(() => fresh.entered, 'second native request held'); await h.pending();
        old.release(); await until(() => old.delivered, 'first response delivered late'); await delay(150);
        await h.pending(); await h.field().press('Enter'); await delay(100);
        assert.equal(h.count('resume_pending'), 2); assert.equal(h.count('prompt'), 0);
        assert(!await h.page.locator('.composer-foot [role=status]').innerText().then(text => text.includes('OLD_ATTEMPT_REJECT')));
        fresh.release(); await until(() => fresh.delivered, 'new attempt accepted'); await h.waiting(); await h.settle();
      });
    }

    await test('14 unresolved native question cannot be bypassed by empty-composer Continue', null, async h => {
      h.fixture.emit({ type: 'agent_start', runId: 'question-failure' });
      h.fixture.emit({ type: 'ui_dialog_request', requestId: 'continue-question', toolCallId: 'question-tool', ui: { purpose: 'question', kind: 'input', question: '仍需确认的问题' } });
      await h.page.locator('[data-request-id="continue-question"]').waitFor();
      h.fixture.emit({ type: 'agent_settled', runId: 'question-failure', outcome: 'failed', pendingResume: true });
      await h.page.locator('[data-session-id="dark"]').filter({ hasText: /失败|待恢复/ }).waitFor();
      await h.noContinue(); await h.field().press('Enter'); await delay(150);
      assert.equal(h.actions().length, 0);
    });
  } finally { if (browser) await browser.close(); }
  if (errors.length || results.some(result => result.status !== 'PASS')) process.exitCode = 1;
})().catch(error => { console.error(error); process.exitCode = 1; });
