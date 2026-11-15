'use strict';

// Formal WASM with isolated node catalogs. Every mutation is captured by origin.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  assert(process.env.WORK_UI_URL && process.env.WORK_UI_BUNDLE, 'Provide the formal bundle and its static URL');
  const fixtures = await Promise.all([startFixture(), startFixture()]);
  const [a, b] = fixtures.map(fixture => new URL(fixture.url).origin);
  const output = path.resolve('artifacts/resource-install-nodes');
  fs.mkdirSync(output, { recursive: true });
  const offer = {
    manifest: { schemaVersion: 1, id: 'test.node-review', name: 'node-review', title: '节点代码审查',
      kind: 'skill', version: '1.0.0', description: '隔离节点安装验证', author: 'Fixture',
      origin: 'fixture', instructions: [] },
    sha256: 'a'.repeat(64), fileCount: 1, sizeBytes: 100, acquired: false,
  };
  const states = new Map([a, b].map(origin => [origin, { acquired: false, resources: [] }]));
  const writes = [], requests = [], errors = [], results = [];
  const authRequired = true;
  let authenticated = false, lostReceipt = false, projectDelay = false, targetCatalogMismatch = false, targetOffline = false;
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const dialog = () => page.getByRole('dialog', { name: '选择安装范围', exact: true });
  const center = () => page.locator('.resource-center-v2');
  const button = (name, root = dialog()) => root.getByRole('button', { name, exact: true });
  const openInstall = async () => {
    await center().locator('.resource-workbench-row').filter({ hasText: '节点代码审查' })
      .getByRole('button', { name: '安装', exact: true }).click();
    await dialog().waitFor();
  };
  const authorize = async (scope, project) => {
    await dialog().locator(`[name=install-scope][value=${scope}]`).check();
    if (project) await dialog().getByLabel('安装项目', { exact: true }).selectOption(project);
    await dialog().getByRole('checkbox').check();
  };
  const ready = async () => {
    await center().waitFor();
    await page.waitForFunction(() => document.querySelector('.resource-center-v2')?.getAttribute('aria-busy') === 'false');
  };
  const sources = new Set([a, b]);
  try {
    await context.route('https://api.github.com/**', route => route.fulfill({ json: {} }));
    await context.route('**/api/v1/**', async route => {
      const request = route.request(), url = new URL(request.url()), origin = url.origin;
      if (!sources.has(origin)) return route.continue();
      const state = states.get(origin);
      const pathname = url.pathname;
      requests.push({ origin, pathname, method: request.method() });
      const send = (json, status = 200) => route.fulfill({ status, json });
      if (pathname === '/api/v1/health' && origin === b && targetOffline) return send({ error: 'Fixture offline' }, 503);
      if (pathname === '/api/v1/auth' && origin === b) {
        if (request.method() === 'POST') {
          assert.equal(request.postDataJSON().password, 'test-only');
          authenticated = true;
        }
        return send({ required: authRequired, authenticated: !authRequired || authenticated });
      }
      if (pathname === '/api/v1/projects' && origin === b) {
        if (projectDelay) await new Promise(resolve => setTimeout(resolve, 600));
        return send({ projects: [
          { id: 'b-project', name: '节点 B 项目', path: '/fixture/b' },
          { id: 'b-other', name: '节点 B 第二项目', path: '/fixture/b2' },
        ] });
      }
      const match = pathname.match(/^\/api\/v1\/projects\/([^/]+)\/(native-resources|packages(?:\/(?:acquire|install))?)$/);
      if (!match) return route.continue();
      const [, project, action] = match;
      const body = request.method() === 'GET' ? null : request.postDataJSON();
      if (body) writes.push({ origin, pathname, body });
      if (action === 'native-resources') {
        assert.equal(request.method(), 'GET', 'This test only installs canonical public resources');
        return send({ profile: { inspect: true, remove: true, install: true, update: true },
          resources: state.resources.filter(r => r.scope === 'global' || r.project === project), diagnostics: [] });
      }
      if (action === 'packages') return send({ source: 'fixture',
        packages: [{ ...offer, acquired: state.acquired,
          sha256: targetCatalogMismatch && origin === b ? 'b'.repeat(64) : offer.sha256 }], diagnostics: [] });
      if (action === 'packages/acquire') {
        state.acquired = true;
        return send({ package: body, source: `/fixture/${project}/node-review` });
      }
      if (action === 'packages/install') {
        assert(state.acquired);
        assert.equal(body.trusted, true);
        assert.deepEqual(body.package, { id: offer.manifest.id, version: offer.manifest.version, sha256: offer.sha256 });
        state.resources.push({ reference: `${origin}-${project}-${body.scope}`, name: offer.manifest.name,
          kind: 'skill', scope: body.scope, project,
          facts: { installed: true, version: offer.manifest.version, source: '/fixture/target-source' } });
        if (lostReceipt) { lostReceipt = false; return send({ error: 'Fixture lost receipt' }, 503); }
        return send({ success: true, observation: {}, diagnostics: '' });
      }
      return route.continue();
    });
    await context.addInitScript(([a, b]) => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, selected: a, recent: [a, b] }));
    }, [a, b]);
    await page.goto(process.env.WORK_UI_URL);
    await page.locator('[data-session-id=dark] .session-item').first().click();
    await page.locator('.primary-nav').getByRole('button', { name: '资源中心', exact: true }).click();
    await ready();
    await openInstall();
    assert.equal(await dialog().getByLabel('安装节点', { exact: true }).inputValue(), a);
    await authorize('project', 'pi-rust');
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(b);
    assert.equal(await dialog().getByRole('checkbox').isChecked(), false);
    assert.equal(await dialog().locator('[name=install-scope]:checked').count(), 0);
    await dialog().getByLabel('目标节点密码', { exact: true }).fill('test-only');
    await button('登录目标节点').click();
    await dialog().locator('[name=install-scope][value=project]').check();
    await dialog().getByLabel('安装项目', { exact: true }).selectOption('b-other');
    assert.deepEqual(await dialog().getByLabel('安装项目', { exact: true }).locator('option').allTextContents(),
      ['节点 B 项目', '节点 B 第二项目']);
    assert(await button('确认安装').isDisabled());
    assert.equal(writes.length, 0);
    pass('node selection clears scope and trust, authenticates only the selected node and loads that node’s projects');

    await dialog().getByRole('checkbox').check();
    lostReceipt = true;
    await button('确认安装').click();
    await dialog().getByRole('alert').waitFor();
    const countBeforeRead = writes.length;
    await button('仅回读安装结果').click();
    await button('完成安装').waitFor();
    assert.equal(writes.length, countBeforeRead);
    assert(writes.every(write => write.origin === b && write.pathname.includes('/b-other/')));
    assert.equal(states.get(a).resources.length, 0);
    await page.screenshot({ path: path.join(output, 'target-confirmed.png') });
    await button('完成安装').click();
    await center().getByRole('status').filter({ hasText: `安装已确认 · ${b}` }).waitFor();
    assert.equal(await center().getByRole('navigation', { name: '资源视图' })
      .getByRole('button', { name: '公共库', exact: true }).getAttribute('aria-pressed'), 'true');
    pass('acquisition, installation and uncertain-receipt recovery remain bound to node B and its selected project');

    projectDelay = true;
    await openInstall();
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(b);
    await dialog().getByRole('status').filter({ hasText: '正在连接目标节点' }).waitFor();
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(a);
    await dialog().locator('[name=install-scope][value=project]').check();
    await page.waitForTimeout(900);
    assert.deepEqual(await dialog().getByLabel('安装项目', { exact: true }).locator('option').evaluateAll(
      options => options.map(option => option.value)), ['rpi-work', 'pi-rust']);
    assert.equal(await dialog().getByLabel('安装节点', { exact: true }).inputValue(), a);
    assert.equal(writes.length, countBeforeRead);
    await button('取消').click();
    projectDelay = false;
    pass('late project reads from a previous node cannot replace the current destination or submit an installation');

    await openInstall();
    await dialog().locator('summary').filter({ hasText: '其他节点' }).click();
    await dialog().getByLabel('安装节点地址', { exact: true }).fill(`${b}/`);
    await button('选择此节点').click();
    await authorize('global');
    await page.setViewportSize({ width: 390, height: 844 });
    const geometry = await dialog().evaluate(element => {
      const box = element.getBoundingClientRect();
      return { left: box.left, right: box.right, width: innerWidth, overflow: element.scrollWidth - element.clientWidth };
    });
    assert(geometry.left >= -1 && geometry.right <= geometry.width + 1 && geometry.overflow <= 1, JSON.stringify(geometry));
    await page.screenshot({ path: path.join(output, 'mobile-target.png') });
    await button('确认安装').click();
    await button('完成安装').click();
    assert.equal(writes.at(-1).body.scope, 'global');
    assert.equal(writes.at(-1).origin, b);
    pass('custom target addresses, global installation and narrow-screen layout work');

    states.get(a).resources.push({ reference: 'a-local-only', name: 'local-only', kind: 'skill',
      scope: 'global', project: 'rpi-work',
      facts: { installed: true, version: '1.0.0', recordedDigest: 'local-only-digest', source: 'D:/node-a/private-source' } });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await center().getByRole('navigation', { name: '资源视图' }).getByRole('button', { name: '已安装', exact: true }).click();
    await ready();
    await center().locator('.resource-workbench-row').filter({ hasText: 'local-only' })
      .getByRole('button', { name: '安装到…', exact: true }).click();
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(b);
    await authorize('global');
    const installedWriteStart = writes.length;
    await button('确认安装').click();
    await dialog().getByRole('alert').filter({ hasText: '目标节点没有此资源的匹配来源' }).waitFor();
    assert.equal(writes.length, installedWriteStart);
    await button('取消').click();
    pass('an original-node source path is never sent to another node when its resource is unavailable there');

    await center().getByRole('navigation', { name: '资源视图' }).getByRole('button', { name: '公共库', exact: true }).click();
    await ready();
    await openInstall();
    targetOffline = true;
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(b);
    await dialog().getByRole('alert').filter({ hasText: '连接失败' }).waitFor();
    assert(await button('确认安装').isDisabled());
    assert.equal(await dialog().getByLabel('安装节点', { exact: true }).inputValue(), b);
    assert.equal(writes.length, installedWriteStart);
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(a);
    assert.equal(await dialog().getByRole('checkbox').isChecked(), false);
    assert.equal(await dialog().locator('[name=install-scope]:checked').count(), 0);
    targetOffline = false;
    await button('取消').click();
    pass('an offline target blocks installation and keeps its destination until the user explicitly selects another node');

    // Both isolated listeners expose the same formal UI assets with their own entry metadata.
    const bundle = path.resolve(process.env.WORK_UI_BUNDLE);
    await context.route('**/releases/**', async route => {
      const url = new URL(route.request().url());
      if (!sources.has(url.origin)) return route.continue();
      const asset = path.resolve(bundle, `.${url.pathname}`);
      assert(asset.startsWith(bundle + path.sep));
      return route.fulfill({ path: asset });
    });
    await context.route('**/.well-known/rpi-work-entry', route => {
      const origin = new URL(route.request().url()).origin;
      return sources.has(origin) ? route.fulfill({ json: { version: 1, mode: 'hosted', nodeId: `node-${origin}` } }) : route.continue();
    });
    await context.route('**/', route => {
      const origin = new URL(route.request().url()).origin;
      return sources.has(origin) ? route.fulfill({ path: path.join(bundle, 'index.html') }) : route.continue();
    });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto(`${a}/`);
    await page.locator('.primary-nav').getByRole('button', { name: '资源中心', exact: true }).click();
    await ready();
    await openInstall();
    await authorize('global');
    const requestStart = requests.length, writeStart = writes.length;
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(b);
    assert.equal(requests.slice(requestStart).filter(request => request.origin === b).length, 0);
    authenticated = false;
    await button('进入目标节点继续安装').click();
    await page.locator('input[type=password]').fill('test-only');
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await dialog().waitFor();
    assert.equal(new URL(page.url()).origin, b);
    assert.equal(await dialog().getByLabel('安装节点', { exact: true }).inputValue(), b);
    assert.equal(await dialog().getByRole('checkbox').isChecked(), false);
    assert.equal(await dialog().locator('[name=install-scope]:checked').count(), 0);
    assert.equal(writes.length, writeStart);
    assert(!page.url().includes('rpi-install'));
    assert(requests.slice(requestStart).filter(request => request.origin === b && request.pathname !== '/api/v1/auth').every(request =>
      request.method === 'GET'), 'The draft never authorizes installation');
    await authorize('project', 'b-project');
    await button('确认安装').click();
    await button('完成安装').waitFor();
    assert(writes.slice(writeStart).every(write => write.origin === b && write.pathname.includes('/b-project/')));
    await button('完成安装').click();
    pass('hosted node navigation resumes exact public identities on node B and requires fresh scope and authorization');

    targetCatalogMismatch = true;
    await page.goto(`${a}/`);
    await page.locator('.primary-nav').getByRole('button', { name: '资源中心', exact: true }).click();
    await ready();
    await openInstall();
    await dialog().getByLabel('安装节点', { exact: true }).selectOption(b);
    const mismatchWriteStart = writes.length;
    await button('进入目标节点继续安装').click();
    await center().getByRole('status').filter({ hasText: '目标节点公共库缺少所选版本或摘要' }).waitFor();
    assert.equal(await dialog().count(), 0);
    assert.equal(writes.length, mismatchWriteStart);
    pass('hosted handoff rejects a target catalog with a different digest and performs no writes');

    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(output, 'validation.json'), JSON.stringify({ results, writes, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: path.join(output, 'failure.png') }).catch(() => {});
    fs.writeFileSync(path.join(output, 'failure.txt'), `${error.stack}\n${await page.locator('body').innerText().catch(() => '')}`);
    throw error;
  } finally {
    await browser.close();
    await Promise.all(fixtures.map(fixture => fixture.close()));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
