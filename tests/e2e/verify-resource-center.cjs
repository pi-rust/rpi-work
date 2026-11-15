'use strict';

// Formal WASM acceptance with isolated, stateful protocol fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const output = path.resolve('artifacts/resource-center');
const bundleUrl = process.env.WORK_UI_URL;
const offers = [
  ['package', 'base', '工作基础'],
  ['package', 'developer', '开发基础'],
  ['skill', 'review', '代码审查'],
  ['extension', 'browser', '浏览器辅助'],
].map(([kind, name, title], index) => ({
  manifest: { schemaVersion: 1, id: `test.${name}`, name, title, kind, version: '1.0.0',
    description: `资源中心的 ${title} 协议测试数据，包含中文与 <img src=x onerror=alert(1)>。`,
    author: 'Isolated test catalog', origin: 'fixture', instructions: kind === 'package' ? ['prompts/system.md'] : [] },
  sha256: String(index + 1).repeat(64), fileCount: 2, sizeBytes: 400, acquired: false,
}));
function native(name, kind, version, scope, project = 'rpi-work', installed = true) {
  return { reference: `${scope}-${project}-${name}-${version}`, name, kind, scope, project,
    facts: { installed, version, recordedDigest: `${name}@${version}`, source: `D:/fixture/${name}`,
      ownership: 'resource', description: `${name} · ${version} 的真实目录字段` } };
}
function desired(target) {
  const ref = reference => ({ reference, identity: reference, diagnostic: null });
  return { mainPackage: ref(target.mainPackage), packageInstructions: target.packageInstructions,
    skills: target.skills.map(ref), extensions: target.extensions.map(ref),
    tools: target.tools, supplementaryInstructions: target.supplementaryInstructions };
}
function observation(target, revision = 1, operations = []) {
  return { configuration: { revision, desired: desired(target), historicalObservation: null },
    operations, readiness: 'verified', executable: true, actual: {}, generation: 'fixture', diagnostic: null };
}
function operation(body, revision) {
  return { retry: body, operationId: body.operationId, expectedRevision: body.expectedRevision,
    target: desired(body.target), supersedesOperationId: null, status: 'applied',
    resultRevision: revision, error: null, generation: 'fixture' };
}
(async () => {
  assert(bundleUrl, 'WORK_UI_URL must point to a fresh formal WASM bundle');
  fs.mkdirSync(output, { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  page.setDefaultTimeout(12000);
  const results = [], errors = [], writes = [], requests = [], geometry = [];
  const pass = text => { console.log('PASS', text); results.push(text); };
  page.on('pageerror', error => errors.push(error.message));
  const resources = [
    native('base', 'package', '1.0.0', 'global'),
    native('base', 'package', '1.0.0', 'project'),
    native('navigator', 'extension', '1.0.0', 'global'),
    native('navigator', 'extension', '1.0.0', 'project'),
    native('navigator', 'extension', '2.0.0', 'project'),
    native('unknown', 'package', '9.0.0', 'global', 'rpi-work', null),
    native('absent', 'skill', '1.0.0', 'project', 'rpi-work', false),
  ];
  const original = { mainPackage: resources[1].reference, packageInstructions: ['original.md'], skills: [], extensions: [],
    tools: { mode: 'only', names: ['read'] }, supplementaryInstructions: '保留原来的会话约定' };
  const sessions = new Map([['dark', observation(original)]]);
  const defaults = new Map();
  const acquired = new Set();
  const applied = new Map();
  let nativeFail = false, installUnknown = false, applyUnknown = false;
  const center = page.locator('.resource-center-v2');
  const button = (name, root = center) => root.getByRole('button', { name, exact: true });
  const row = name => center.locator('.resource-workbench-row').filter({ has: page.locator('h2', { hasText: name }) });
  const totals = () => center.locator('.resource-center-overview strong').allTextContents();
  const ready = async () => page.waitForFunction(() => document.querySelector('.resource-center-v2')?.getAttribute('aria-busy') === 'false');
  const source = async name => { await button(name, center.getByRole('navigation', { name: '资源视图' })).click(); await ready(); };
  const scope = async value => { await center.getByLabel('已安装资源范围', { exact: true }).selectOption(value); await ready(); };
  const modal = name => page.getByRole('dialog', { name, exact: true });
  const select = async name => {
    const choose = row(name).first().getByRole('button', { name: '选择', exact: true });
    if (await choose.count()) await choose.click();
  };
  const install = async (scopeValue, project) => {
    const dialog = modal('选择安装范围');
    await dialog.locator(`input[name=install-scope][value=${scopeValue}]`).check();
    if (project) await dialog.getByLabel('安装项目', { exact: true }).selectOption(project);
    await dialog.getByRole('checkbox').check();
    await button('确认安装', dialog).click();
    return dialog;
  };
  const noOverflow = async () => {
    const metrics = await page.evaluate(() => {
      const root = document.querySelector('.resource-center-v2');
      const dialogs = [...document.querySelectorAll('dialog[open]')];
      return { width: innerWidth, root: root.scrollWidth - root.clientWidth,
        document: document.documentElement.scrollWidth - innerWidth,
        dialogs: dialogs.map(dialog => {
          const box = dialog.getBoundingClientRect();
          return { overflow: dialog.scrollWidth - dialog.clientWidth, x: box.x, right: box.right };
        }) };
    });
    geometry.push(metrics);
    assert(metrics.root <= 1 && metrics.document <= 1, JSON.stringify(metrics));
    assert(metrics.dialogs.every(dialog => dialog.overflow <= 1 && dialog.x >= -1 && dialog.right <= metrics.width + 1), JSON.stringify(metrics));
  };
  try {
    await page.route('https://api.github.com/**', route => route.fulfill({ json: {} }));
    await page.route('**/api/v1/**', async route => {
      const request = route.request();
      const pathname = new URL(request.url()).pathname;
      const match = pathname.match(/^\/api\/v1\/projects\/([^/]+)\/(native-resources|packages(?:\/(?:bundle|acquire|install))?|native-resource-config|resource-sessions)$/);
      const sessionMatch = pathname.match(/^\/api\/v1\/sessions\/([^/]+)\/resources$/);
      if (!match && !sessionMatch) return route.continue();
      let body;
      try { body = request.postDataJSON(); } catch { body = null; }
      requests.push({ method: request.method(), pathname });
      if (request.method() !== 'GET') writes.push({ pathname, body });
      const send = (json, status = 200) => route.fulfill({ status, json });
      if (sessionMatch) {
        const id = sessionMatch[1];
        if (request.method() === 'GET') return send(sessions.get(id) || observation(original));
        let value = applied.get(body.operationId);
        if (!value) {
          value = operation(body, (sessions.get(id)?.configuration.revision || 0) + 1);
          applied.set(body.operationId, value);
          sessions.set(id, observation(body.target, value.resultRevision, [value]));
        }
        if (applyUnknown) { applyUnknown = false; return send({ error: 'fixture receipt unavailable' }, 503); }
        return send(value);
      }
      const [, project, action] = match;
      if (action === 'native-resources') {
        if (request.method() === 'GET') {
          if (nativeFail) return send({ error: 'fixture catalog unavailable' }, 503);
          return send({ profile: { inspect: true, remove: true, install: true, update: true },
            resources: resources.filter(r => r.scope === 'global' || r.project === project), diagnostics: [] });
        }
        if (body.operation === 'remove') {
          const index = resources.findIndex(resource => resource.reference === body.reference);
          if (index >= 0) resources.splice(index, 1);
        }
        return send({ success: true, observation: { operation: body.operation }, diagnostics: '' });
      }
      if (action === 'packages') return send({ source: 'isolated fixture catalog',
        packages: offers.map(offer => ({ ...offer, acquired: acquired.has(offer.manifest.id) })), diagnostics: [] });
      if (action === 'packages/bundle') {
        const offer = offers.find(offer => offer.manifest.id === body.id);
        return send({ manifest: offer.manifest, sources: { 'SKILL.md': '# Test\n内容 <img src=x>' }, sha256: offer.sha256 });
      }
      if (action === 'packages/acquire') {
        acquired.add(body.id);
        return send({ package: body, source: `D:/fixture/${body.id}` });
      }
      if (action === 'packages/install') {
        assert.equal(body.trusted, true);
        assert(acquired.has(body.package.id));
        const offer = offers.find(offer => offer.manifest.id === body.package.id);
        if (!resources.some(resource => resource.name === offer.manifest.name && resource.scope === body.scope && (resource.scope === 'global' || resource.project === project))) {
          resources.push(native(offer.manifest.name, offer.manifest.kind, offer.manifest.version, body.scope, project));
        }
        if (installUnknown) { installUnknown = false; return send({ error: 'fixture lost receipt' }, 503); }
        return send({ success: true, observation: {}, diagnostics: '' });
      }
      if (action === 'native-resource-config') {
        if (request.method() === 'GET') return send(defaults.get(project) || { revision: 0, selection: null, diagnostic: null });
        const revision = (defaults.get(project)?.revision || 0) + 1;
        defaults.set(project, { revision, selection: body.target, diagnostic: null });
        return send(operation(body, revision));
      }
      if (action === 'resource-sessions') {
        const response = await page.request.post(new URL('api/v1/sessions', fixture.url).href, { data: { projectId: project } });
        assert(response.ok(), 'fixture session creation must succeed');
        const { sessionId } = await response.json();
        sessions.set(sessionId, observation(body.selection));
        return send({ sessionId });
      }
      return route.continue();
    });
    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      localStorage.setItem('rpi-work-settings-v1', JSON.stringify({ version: 1, appearance: { theme: 'codex', mode: 'system', density: 'standard', toolRendering: 'simple', customThemes: [] } }));
    }, fixture.url);
    await page.goto(bundleUrl);
    await page.locator('[data-session-id=dark] .session-item').click();
    await page.locator('[data-history-id=user-dark]').waitFor();
    const draft = page.locator('.composer textarea');
    await draft.fill('原会话的资源工作草稿');
    const navigation = page.locator('.primary-nav');
    assert.equal(await navigation.getByRole('button').count(), 2);
    await button('资源中心', navigation).click();
    await ready();
    assert.equal(await center.locator('[data-lucide="circle-help"]').count(), 0, 'all resource icons use bundled geometry');
    assert.deepEqual(await totals(), ['01', '01', '02']);
    assert.equal(await center.locator('.resource-workbench-row').count(), 4);
    assert.equal(writes.length, 0);
    await center.getByLabel('搜索资源', { exact: true }).fill('代码审查');
    assert.deepEqual(await totals(), ['01', '01', '02']);
    assert.equal(await center.locator('.resource-workbench-row').count(), 1);
    assert.match(await center.locator('.resource-result-count').innerText(), /显示 1 \/ 共 4/);
    assert.equal(await center.locator('img').count(), 0);
    await center.getByLabel('搜索资源', { exact: true }).fill('');
    pass('single resource center defaults to the public catalog with source counts independent of search and escaped metadata');

    await source('已安装');
    assert.deepEqual(await totals(), ['01', '00', '01']);
    await scope('rpi-work');
    assert.deepEqual(await totals(), ['02', '00', '01']);
    assert.equal(await center.locator('.native-resource-card').count(), 3);
    assert.equal(await row('navigator').count(), 2, 'different versions remain visible');
    assert.match(await row('base').innerText(), /项目安装 · 全局也可用/);
    await scope('pi-rust');
    assert.deepEqual(await totals(), ['01', '00', '01']);
    await scope('rpi-work');
    await row('base').getByRole('button', { name: '查看详情', exact: true }).click();
    const detail = modal('资源详情');
    await detail.locator('summary').filter({ hasText: '管理此资源' }).click();
    await button('移除资源', detail).click();
    await button('取消', detail.getByRole('region', { name: '移除确认' })).click();
    assert.equal(writes.length, 0);
    await button('移除资源', detail).click();
    await button('确认移除', detail).click();
    await detail.locator('.native-detail-notice').waitFor();
    await page.keyboard.press('Escape');
    await ready();
    assert.deepEqual(await totals(), ['02', '00', '01']);
    await row('base').filter({ hasText: /全局可用/ }).waitFor();
    assert.equal(writes.at(-1).body.reference, 'project-rpi-work-base-1.0.0');
    pass('installed scopes inherit globals, deduplicate matching versions, preserve distinct versions and remove only the explicit reference');

    await source('公共库');
    await row('代码审查').getByRole('button', { name: '安装', exact: true }).click();
    let dialog = modal('选择安装范围');
    assert(await button('确认安装', dialog).isDisabled());
    await dialog.locator('[name=install-scope][value=global]').check();
    const writesBeforeCancel = writes.length;
    await button('取消', dialog).click();
    assert.equal(writes.length, writesBeforeCancel);
    await row('代码审查').getByRole('button', { name: '安装', exact: true }).click();
    installUnknown = true;
    dialog = await install('global');
    await dialog.getByRole('alert').waitFor();
    assert(!await dialog.getByRole('checkbox').isChecked());
    const installsBeforeReadback = writes.filter(entry => entry.pathname.endsWith('/packages/install')).length;
    await button('仅回读安装结果', dialog).click();
    await button('完成安装', dialog).click();
    await ready();
    assert.equal(writes.filter(entry => entry.pathname.endsWith('/packages/install')).length, installsBeforeReadback);
    assert.deepEqual(await totals(), ['01', '01', '01']);
    await scope('pi-rust');
    assert.deepEqual(await totals(), ['01', '01', '01']);
    assert.equal(await center.locator('.resource-selected-item').count(), 0);
    pass('install modal requires explicit scope and trust, cancellation is inert, lost receipts recover by readback without reinstalling');

    await source('公共库');
    await row('浏览器辅助').getByRole('button', { name: '安装', exact: true }).click();
    dialog = await install('project', 'pi-rust');
    await button('完成安装', dialog).click();
    await ready();
    assert.equal(await center.getByLabel('已安装资源范围', { exact: true }).inputValue(), 'pi-rust');
    assert.deepEqual(await totals(), ['02', '01', '01']);
    await scope('global');
    assert.deepEqual(await totals(), ['01', '01', '01']);
    pass('project installation affects the chosen project and keeps global counts unchanged');

    await source('公共库');
    await select('工作基础');
    await select('代码审查');
    await select('开发基础');
    await button('取消', modal('替换主 Package？')).click();
    assert.equal(await center.locator('.resource-selected-item').count(), 2);
    const selection = center.locator('.resource-selection-panel');
    await button('开始工作', selection).click();
    let work = modal('开始工作');
    await work.getByLabel('工作会话', { exact: true }).selectOption('current');
    applyUnknown = true;
    await button('准备并继续', work).click();
    await work.getByRole('alert').waitFor();
    await button('回读并重试原操作', work).click();
    await work.waitFor({ state: 'hidden' });
    await page.waitForFunction(expected => document.querySelector('.composer textarea')?.value === expected, '原会话的资源工作草稿');
    assert.equal(await draft.inputValue(), '原会话的资源工作草稿');
    const applies = writes.filter(entry => entry.pathname === '/api/v1/sessions/dark/resources');
    assert.equal(applies.length, 2);
    assert.equal(applies[0].body.operationId, applies[1].body.operationId);
    assert.deepEqual(applies[0].body.target.tools, original.tools);
    assert.equal(applies[0].body.target.supplementaryInstructions, original.supplementaryInstructions);
    pass('cross-type selection keeps one Package, applies to the existing session, retains configuration/draft and retries with the same operation ID');

    await button('资源中心', navigation).click();
    await ready();
    await select('开发基础');
    await select('浏览器辅助');
    await button('开始工作', selection).click();
    work = modal('开始工作');
    await button('准备并继续', work).click();
    dialog = modal('选择安装范围');
    await button('返回工作设置', dialog).click();
    await work.getByLabel('工作项目', { exact: true }).selectOption('rpi-work');
    await button('准备并继续', work).click();
    dialog = await install('project');
    await button('继续工作准备', dialog).click();
    await button('准备并继续', work).click();
    await work.waitFor({ state: 'hidden' });
    assert.equal(writes.filter(entry => entry.pathname.endsWith('/resource-sessions')).length, 1);
    assert.deepEqual(writes.find(entry => entry.pathname.endsWith('/resource-sessions')).body.selection.packageInstructions, ['prompts/system.md']);
    pass('missing tools enter the same installation modal and continue to exactly one new resource-configured session');

    await button('资源中心', navigation).click();
    await ready();
    await select('工作基础');
    await select('代码审查');
    await button('设为项目默认组合', selection).click();
    work = modal('设置项目默认组合');
    await button('准备并继续', work).click();
    await work.getByRole('status').filter({ hasText: '已保存并回读确认' }).waitFor();
    assert.equal(defaults.get('rpi-work').selection.skills.length, 1);
    await button('完成', work).click();
    pass('project defaults are saved through the typed API and confirmed by readback');

    await source('已安装');
    await scope('rpi-work');
    nativeFail = true;
    await button('刷新目录 / 回读').click();
    await center.getByRole('alert').waitFor();
    assert.deepEqual(await totals(), ['—', '—', '—']);
    assert.equal(await center.locator('.resource-center-overview small').allTextContents().then(values => values.every(value => value === '数量未核实')), true);
    nativeFail = false;
    await button('刷新目录 / 回读').click();
    await ready();
    await button('导入设计').click();
    await modal('导入设计包').waitFor();
    await page.keyboard.press('Escape');
    await button('开发工具').click();
    await modal('资源开发').waitFor();
    await page.keyboard.press('Escape');
    pass('read failures clear stale counts, design import and node authoring remain reachable');

    for (const sourceName of ['公共库', '已安装']) {
      await source(sourceName);
      for (const width of [1920, 1440, 1024, 768, 390, 320]) {
        await page.setViewportSize({ width, height: width < 768 ? 844 : 1000 });
        await center.locator('.resource-center-heading').scrollIntoViewIfNeeded();
        if (width < 768) assert.match(await page.locator('.page-breadcrumb').innerText(), /资源中心/);
        await noOverflow();
        await page.screenshot({ path: path.join(output, `${sourceName}-${width}.png`), animations: 'disabled' });
      }
    }
    await row('base').first().getByRole('button', { name: '安装到…', exact: true }).click();
    dialog = modal('选择安装范围');
    await dialog.locator('[name=install-scope][value=project]').check();
    await noOverflow();
    await page.screenshot({ path: path.join(output, 'install-320.png'), animations: 'disabled' });
    await page.keyboard.press('Escape');
    assert.deepEqual(errors, []);
    pass('resource center and installation dialog fit 1920–320px without horizontal overflow or script errors');
  } catch (error) {
    process.exitCode = 1;
    errors.push(error.stack);
    console.error(error);
    await page.screenshot({ path: path.join(output, 'failure.png') }).catch(() => {});
  } finally {
    fs.writeFileSync(path.join(output, 'validation.json'), JSON.stringify({ passed: !process.exitCode, results, errors, writes, requests, geometry }, null, 2));
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
