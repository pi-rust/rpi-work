// Formal Web/WASM + isolated HTTP/SSE fixtures only; no native GUI or model calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const TIMEOUT = 15000;
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
async function bounded(promise, label) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`Timed out: ${label}`)), TIMEOUT);
    })]);
  } finally { clearTimeout(timer); }
}

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const results = [], errors = [], gates = [], contexts = [], pages = [];
  let a, b, browser;
  const pass = name => { results.push(name); console.log('PASS', name); };
  const project = (page, id) => page.locator(`.project-entry[data-project-id="${id}"]`);
  const session = (page, id) => page.locator(`.session-entry[data-session-id="${id}"]`);
  const composer = page => page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
  const heading = (page, name) => page.locator('.app-header').getByRole('heading', { name, exact: true });
  const listURL = (node, kind) => url => url.origin === new URL(node.url).origin && url.pathname === `/api/v1/${kind}`;
  const frames = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const draft = '被动端尚未发送的草稿：中文 / emoji 🐟';
  let observerScope;

  async function openNodes(page) {
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
  }
  async function connect(page, node, requireLogin = false) {
    await openNodes(page);
    if (!await page.locator('#node-url').isVisible()) {
      await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    }
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    const status = page.locator('.server-details [role=status]');
    if (requireLogin) {
      await status.filter({ hasText: '需要节点密码' }).waitFor();
      await page.getByLabel('节点密码', { exact: true }).fill('test-only');
      await page.getByRole('button', { name: '登录', exact: true }).click();
    }
    await status.filter({ hasText: '在线' }).waitFor();
    assert.equal(await page.locator('.node-address code').innerText(), node.url);
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await project(page, 'rpi-work').waitFor();
  }
  async function expand(page, id) {
    const toggle = project(page, id).locator('.project-toggle');
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await page.waitForFunction(id => document.querySelector(`.project-entry[data-project-id="${id}"] .project-toggle`)?.getAttribute('aria-expanded') === 'true', id);
  }
  async function chooseDark(page, title = '为工作台添加深色模式') {
    await expand(page, 'rpi-work');
    await session(page, 'dark').locator('.session-item').click();
    await heading(page, title).waitFor();
    await page.waitForFunction(() => {
      const field = document.querySelector('.composer textarea');
      return field && !field.disabled && document.querySelector('.transcript')?.dataset.sessionScope?.endsWith('::session::dark');
    });
  }
  async function retained(page) {
    await heading(page, '为工作台添加深色模式').waitFor();
    assert.equal(await composer(page).inputValue(), draft, 'passive refresh must retain the unsent draft');
    assert.equal(await session(page, 'dark').locator('.session-item').getAttribute('aria-current'), 'true');
    assert.equal(await page.locator('.session-entry.selected').count(), 1);
    assert.equal(await page.locator('.transcript').getAttribute('data-session-scope'), observerScope);
    assert(await page.evaluate(() => window.__workspaceSyncDocument === 'retained'), 'sync must not reload the document');
  }
  async function sessionAction(page, id, action) {
    await session(page, id).locator('.session-row-menu').click();
    const menu = page.getByRole('menu', { name: '会话操作', exact: true });
    await menu.waitFor();
    await menu.getByRole('menuitem', { name: action, exact: true }).click();
  }
  async function renameSession(page, id, title) {
    await sessionAction(page, id, '重命名');
    const dialog = page.getByRole('dialog', { name: '重命名会话', exact: true });
    await dialog.getByLabel('会话名称', { exact: true }).fill(title);
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
    await session(page, id).getByText(title, { exact: true }).waitFor();
  }
  async function renameProject(page, id, name) {
    await project(page, id).locator('.project-row-menu').click();
    await page.getByRole('menuitem', { name: '重命名项目', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '项目名称与目录', exact: true });
    await dialog.getByLabel('显示名称', { exact: true }).fill(name);
    await dialog.getByLabel('显示名称', { exact: true }).press('Enter');
    await dialog.waitFor({ state: 'hidden' });
    await project(page, id).locator('.project-row').getByText(name, { exact: true }).waitFor();
  }
  async function api(context, node, path, data) {
    const response = await context.request.post(new URL(`api/v1/${path}`, node.url).href, { data });
    assert(response.ok(), `${path}: HTTP ${response.status()}`);
    return response.json();
  }
  // Hold an actual GET response snapshot, not a fabricated list. Only the first
  // GET is delayed, so reconnects can load authoritative newer data normally.
  async function gateList(context, node, kind) {
    const arrived = deferred(), release = deferred(), done = deferred();
    let used = false;
    const pattern = listURL(node, kind);
    const handler = async route => {
      if (used || route.request().method() !== 'GET') return route.continue();
      used = true;
      try {
        const response = await route.fetch();
        const payload = await response.json();
        arrived.resolve(payload);
        await release.promise;
        try { await route.fulfill({ response }); }
        catch (error) {
          // Scope changes may cancel browser fetches; cancellation is safe too.
          if (!/closed|disposed|invalid interception|not found|cancel|finished/i.test(error.message)) throw error;
        }
      } catch (error) { errors.push(`delayed ${kind}: ${error.message}`); arrived.resolve(null); }
      finally { done.resolve(); }
    };
    await context.route(pattern, handler);
    const gate = {
      release,
      arrived: () => bounded(arrived.promise, `background ${kind} GET`),
      finish: async () => {
        release.resolve();
        await bounded(done.promise, `delayed ${kind} delivery`);
        await context.unroute(pattern, handler);
      },
    };
    gates.push(gate);
    return gate;
  }
  async function nextList(page, node, kind) {
    const response = await page.waitForResponse(response => {
      const url = new URL(response.url());
      return listURL(node, kind)(url) && response.request().method() === 'GET' && response.ok();
    }, { timeout: TIMEOUT });
    await response.finished();
    await frames(page);
  }
  // Observe transient stale writes as well as the final DOM; a later poll must
  // not hide a brief replacement by a late response.
  async function watchForbidden(page, selectors) {
    await page.evaluate(selectors => {
      window.__workspaceSyncWatcher?.disconnect();
      window.__workspaceSyncStale = [];
      const check = () => {
        for (const { selector, text } of selectors) {
          const elements = [...document.querySelectorAll(selector)];
          if (elements.some(el => text === undefined || el.textContent.includes(text))) {
            window.__workspaceSyncStale.push({ selector, text });
          }
        }
      };
      window.__workspaceSyncWatcher = new MutationObserver(check);
      window.__workspaceSyncWatcher.observe(document.body, { childList: true, subtree: true, characterData: true });
      check();
    }, selectors);
  }
  async function noStale(page) {
    await frames(page);
    assert.deepEqual(await page.evaluate(() => window.__workspaceSyncStale), [], 'late scope response reached the directory DOM');
    await page.evaluate(() => window.__workspaceSyncWatcher.disconnect());
  }

  try {
    a = await startFixture(0, { authRequired: true });
    b = await startFixture();
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    for (const name of ['actor', 'observer']) {
      const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
      contexts.push(context);
      await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}', headers: { 'access-control-allow-origin': '*' } }));
      const page = await context.newPage();
      pages.push(page);
      page.setDefaultTimeout(TIMEOUT);
      page.on('pageerror', error => errors.push(`${name}: ${error.message}`));
      await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
      // Each fresh context must authenticate through the UI, not copied cookies.
      await connect(page, a, true);
    }
    const [actor, observer] = pages;
    const [actorContext, observerContext] = contexts;
    assert.notEqual(actor.context(), observer.context());
    assert.equal(a.requests.filter(r => r.path === '/api/v1/auth' && r.method === 'POST').length, 2);
    assert(a.requests.some(r => r.path === '/api/v1/projects' && r.authenticated));
    await api(actorContext, b, 'projects/rpi-work/rename', { name: '节点 B 工作空间' });
    await api(actorContext, b, 'sessions/dark/rename', { title: '节点 B 的独立会话' });
    await chooseDark(actor);
    await chooseDark(observer);
    await composer(observer).fill(draft);
    observerScope = await observer.locator('.transcript').getAttribute('data-session-scope');
    await observer.evaluate(() => { window.__workspaceSyncDocument = 'retained'; });
    pass('two independent browser contexts connect and log in to the same isolated password-protected fixture');

    // No passive-side navigation, focus signal, reload, or manual refresh here:
    // these changes must be discovered by the ordinary three-second poll.
    const createdResponse = actor.waitForResponse(r => r.url() === `${a.url}api/v1/sessions` && r.request().method() === 'POST' && r.ok());
    await project(actor, 'rpi-work').getByRole('button', { name: '在 rpi-work 中新建会话', exact: true }).click();
    const { sessionId: createdId } = await (await createdResponse).json();
    assert(createdId);
    await heading(actor, '新会话').waitFor();
    await session(observer, createdId).locator('.session-item').waitFor();
    await retained(observer);
    await renameSession(actor, createdId, '跨客户端新会话');
    await session(observer, createdId).getByText('跨客户端新会话', { exact: true }).waitFor();
    await retained(observer);
    pass('UI session creation and rename appear automatically on the passive client without changing its conversation or draft');

    await sessionAction(actor, createdId, '置顶');
    await session(actor, createdId).getByText('置顶', { exact: true }).waitFor();
    await session(observer, createdId).getByText('置顶', { exact: true }).waitFor();
    assert.equal(await project(observer, 'rpi-work').locator('.session-entry').first().getAttribute('data-session-id'), createdId);
    await retained(observer);
    await sessionAction(actor, createdId, '归档会话');
    await session(actor, createdId).waitFor({ state: 'hidden' });
    await session(observer, createdId).waitFor({ state: 'hidden' });
    await retained(observer);
    pass('pin metadata/order and archive visibility synchronize without resetting the passive selection or draft');

    // Open/register a directory only on the in-memory fixture; this path never
    // touches a real filesystem and does not invoke native directory pickers.
    await actor.getByRole('button', { name: '添加项目', exact: true }).click();
    const add = actor.getByRole('dialog', { name: '添加项目', exact: true });
    await add.getByLabel('已有项目目录', { exact: true }).fill('D:/Projects/fixture-workspace-sync-only');
    await add.locator('#project-picker-name').fill('跨端新增项目');
    const addedResponse = actor.waitForResponse(r => r.url() === `${a.url}api/v1/projects` && r.request().method() === 'POST' && r.ok());
    await add.getByRole('button', { name: '打开项目', exact: true }).click();
    const { project: added } = await (await addedResponse).json();
    assert(added?.id);
    await add.waitFor({ state: 'hidden' });
    await project(observer, added.id).locator('.project-row').getByText('跨端新增项目', { exact: true }).waitFor();
    await retained(observer);
    await renameProject(actor, added.id, '跨端重命名项目');
    await project(observer, added.id).locator('.project-row').getByText('跨端重命名项目', { exact: true }).waitFor();
    await retained(observer);
    pass('UI project registration and project rename propagate automatically while the passive composer remains intact');

    // Browser event simulation (not native window acceptance). First observe a
    // completed poll to avoid mistaking an already-due periodic refresh for the
    // event's immediate refresh. Its next response must arrive before 3 seconds.
    for (const event of ['visibilitychange', 'focus']) {
      await nextList(observer, a, 'sessions');
      const update = event === 'focus' ? '聚焦后的会话名称' : '重新可见后的会话名称';
      await expand(actor, 'rpi-work');
      await renameSession(actor, 'architecture', update);
      const refreshed = observer.waitForResponse(response => {
        const url = new URL(response.url());
        return listURL(a, 'sessions')(url) && response.request().method() === 'GET' && response.ok();
      }, { timeout: 2000 });
      await observer.evaluate(event => {
        if (event === 'visibilitychange') {
          // Headless contexts are already visible; explicitly model hidden -> visible.
          Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
          Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' });
          document.dispatchEvent(new Event('visibilitychange'));
          delete document.hidden;
          delete document.visibilityState;
          document.dispatchEvent(new Event('visibilitychange'));
        } else window.dispatchEvent(new Event('focus'));
      }, event);
      const response = await refreshed;
      await response.finished();
      await session(observer, 'architecture').getByText(update, { exact: true }).waitFor();
      await retained(observer);
    }
    pass('visible/focus browser signals refresh the directory and preserve the passive conversation and draft');

    // Fail both list endpoints on the passive context only. Arm the route before
    // mutation, prove an unsuccessful background request, retain the old rows,
    // then remove the failure and wait for a normal scheduled retry (no reload).
    const failed = { projects: deferred(), sessions: deferred() };
    let failureMode = true;
    const failLists = async route => {
      if (route.request().method() !== 'GET' || !failureMode) return route.continue();
      const kind = new URL(route.request().url()).pathname.split('/').at(-1);
      await route.fulfill({ status: 503, json: { error: 'Isolated directory refresh failure' }, headers: { 'access-control-allow-origin': new URL(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/').origin, 'access-control-allow-credentials': 'true' } });
      failed[kind].resolve();
    };
    const failingPattern = url => url.origin === new URL(a.url).origin && /^\/api\/v1\/(projects|sessions)$/.test(url.pathname);
    await observerContext.route(failingPattern, failLists);
    await renameSession(actor, 'architecture', '失败后自动重试成功');
    await renameProject(actor, added.id, '项目刷新重试成功');
    // Some implementations stop a combined refresh on the first failure. One
    // observed failed endpoint is sufficient to exercise that failed round.
    await bounded(Promise.race([failed.projects.promise, failed.sessions.promise]), 'failed background directory refresh');
    await frames(observer);
    assert(await project(observer, added.id).isVisible(), 'failed refresh must not empty the project list');
    assert(await session(observer, 'architecture').isVisible(), 'failed refresh must not empty the session list');
    await retained(observer);
    failureMode = false;
    await observerContext.unroute(failingPattern, failLists);
    await session(observer, 'architecture').getByText('失败后自动重试成功', { exact: true }).waitFor();
    await project(observer, added.id).locator('.project-row').getByText('项目刷新重试成功', { exact: true }).waitFor();
    await retained(observer);
    pass('a failed background list refresh retains existing rows and automatically retries to discover newer data');

    // A slow list must not hold a successful response from the other endpoint.
    // Pause the browser clock before mutation, then resume into one known round.
    for (const stalled of ['sessions', 'projects']) {
      await nextList(observer, a, 'projects');
      await observer.evaluate(() => {
        Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
        document.dispatchEvent(new Event('visibilitychange'));
      });
      const gate = await gateList(observerContext, a, stalled);
      const updated = `另一目录不被 ${stalled} 阻塞`;
      if (stalled === 'sessions') await renameProject(actor, added.id, updated);
      else await renameSession(actor, 'architecture', updated);
      await observer.evaluate(() => {
        delete document.hidden;
        document.dispatchEvent(new Event('visibilitychange'));
      });
      await gate.arrived();
      const target = stalled === 'sessions'
        ? project(observer, added.id).locator('.project-row').getByText(updated, { exact: true })
        : session(observer, 'architecture').getByText(updated, { exact: true });
      await target.waitFor({ timeout: 2000 });
      await retained(observer);
      await gate.finish();
    }
    pass('a stalled projects or sessions endpoint cannot block the other successful directory response');

    // Local acknowledgements must also win over an older poll in the same epoch.
    for (const kind of ['sessions', 'projects']) {
      const gate = await gateList(observerContext, a, kind);
      const snapshot = await gate.arrived();
      const oldName = kind === 'sessions'
        ? snapshot.sessions.find(s => s.sessionId === 'architecture').title
        : snapshot.projects.find(p => p.id === added.id).name;
      const name = `本端已确认的 ${kind} 修改`;
      if (kind === 'sessions') await renameSession(observer, 'architecture', name);
      else await renameProject(observer, added.id, name);
      await watchForbidden(observer, [{ selector: kind === 'sessions' ? '.session-title' : '.project-row', text: oldName }]);
      await gate.finish();
      await nextList(observer, a, kind);
      await noStale(observer);
      const target = kind === 'sessions'
        ? session(observer, 'architecture').getByText(name, { exact: true })
        : project(observer, added.id).locator('.project-row').getByText(name, { exact: true });
      await target.waitFor();
      await retained(observer);
    }
    pass('late polls cannot overwrite locally acknowledged renames in the same connection epoch');

    // Exercise each directory response independently, including scope changes.
    for (const kind of ['sessions', 'projects']) {
      const gate = await gateList(observerContext, a, kind);
      const snapshot = await gate.arrived();
      assert(snapshot && Array.isArray(snapshot[kind]), `must capture real ${kind} payload`);
      await connect(observer, b);
      await chooseDark(observer, '节点 B 的独立会话');
      await composer(observer).fill(`节点 B 的 ${kind} 草稿`);
      await watchForbidden(observer, [
        { selector: `.project-entry[data-project-id="${added.id}"]` },
        { selector: '.session-title', text: '失败后自动重试成功' },
      ]);
      await gate.finish();
      await nextList(observer, b, kind);
      await noStale(observer);
      await heading(observer, '节点 B 的独立会话').waitFor();
      assert.equal(await composer(observer).inputValue(), `节点 B 的 ${kind} 草稿`);
      assert.equal(await session(observer, 'dark').locator('.session-item').getAttribute('aria-current'), 'true');
      assert.equal(await project(observer, added.id).count(), 0);
      await project(observer, 'rpi-work').locator('.project-row').getByText('节点 B 工作空间', { exact: true }).waitFor();
      await connect(observer, a);
      await chooseDark(observer);
    }
    pass('late projects and sessions responses from node A cannot write into node B, even with shared seed IDs');

    // Same URL is not enough: reconnect must advance the epoch and reject an
    // old successful snapshot even though its node URL matches the current one.
    for (const kind of ['sessions', 'projects']) {
      const gate = await gateList(observerContext, a, kind);
      const snapshot = await gate.arrived();
      assert(snapshot && Array.isArray(snapshot[kind]));
      const oldName = kind === 'sessions'
        ? snapshot.sessions.find(s => s.sessionId === 'architecture').title
        : snapshot.projects.find(p => p.id === added.id).name;
      const newName = `同地址新 epoch ${kind}`;
      if (kind === 'sessions') await renameSession(actor, 'architecture', newName);
      else await renameProject(actor, added.id, newName);
      await connect(observer, a);
      await chooseDark(observer);
      await composer(observer).fill(`新 epoch ${kind} 草稿`);
      const target = kind === 'sessions' ? session(observer, 'architecture').getByText(newName, { exact: true }) : project(observer, added.id).locator('.project-row').getByText(newName, { exact: true });
      await target.waitFor();
      await watchForbidden(observer, [{ selector: kind === 'sessions' ? '.session-title' : '.project-row', text: oldName }]);
      await gate.finish();
      await nextList(observer, a, kind);
      await noStale(observer);
      assert(await target.isVisible());
      assert.equal(await composer(observer).inputValue(), `新 epoch ${kind} 草稿`);
      assert.equal(await session(observer, 'dark').locator('.session-item').getAttribute('aria-current'), 'true');
    }
    pass('same-address reconnect rejects stale directory epochs without reverting names, selection, or the new draft');

    const finalDraft = await composer(observer).inputValue();
    await renameSession(actor, 'dark', '当前会话也接受跨端改名');
    await heading(observer, '当前会话也接受跨端改名').waitFor();
    assert.equal(await composer(observer).inputValue(), finalDraft);
    assert.equal(await session(observer, 'dark').locator('.session-item').getAttribute('aria-current'), 'true');
    pass('a remotely renamed current conversation updates its header without replacing selection or draft');

    assert(await observer.evaluate(() => window.__workspaceSyncDocument === 'retained'), 'no scenario may globally reload the page');
    for (const node of [a, b]) {
      assert.equal(node.requests.filter(r => r.method === 'DELETE').length, 0);
      assert.equal(node.requests.filter(r => ['prompt', 'queue_message', 'set_model', 'set_thinking_level', 'set_api_key'].includes(r.body.type || r.body.command)).length, 0);
    }
    assert.deepEqual(errors, []);
    await observer.screenshot({ path: 'artifacts/workspace-sync.png', animations: 'disabled' });
    pass('no document reload, destructive DELETE, prompt/queue/model/credential mutation, or browser application error');
  } catch (error) {
    errors.push(error.message);
    for (let index = 0; index < pages.length; index++) {
      await pages[index].screenshot({ path: `artifacts/workspace-sync-failure-${index}.png`, animations: 'disabled' }).catch(() => {});
    }
    throw error;
  } finally {
    for (const gate of gates) gate.release.resolve();
    fs.writeFileSync('artifacts/workspace-sync-validation.json', JSON.stringify({
      scope: 'Formal Web/WASM, two independent browser contexts, isolated HTTP/SSE fixtures; simulated visibility/focus; no native GUI, real node/user data, or model calls',
      results, errors,
    }, null, 2));
    await browser?.close();
    await Promise.all([a?.close(), b?.close()]);
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
