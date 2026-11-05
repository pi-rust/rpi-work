'use strict';

// Fixture-only real WASM regression. The caller must point WORK_UI_URL at a freshly
// built WASM web bundle; this deliberately does not fall back to the old preview.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

const bundleUrl = process.env.WORK_UI_URL;
const outputDir = path.resolve('artifacts');
const reportPath = path.join(outputDir, 'resource-center-validation.json');

const nativeResources = [
  {
    reference: 'project/package/base-tools', kind: 'package', scope: 'project',
    name: '基础工作资源包',
    facts: {
      installed: true, inUse: false, ownership: 'resource',
      source: 'D:/Projects/rpi-work/.rpi/resources/package/base-tools/entry.json',
      version: '1.2.3', recordedDigest: 'a'.repeat(64),
    },
  },
  {
    reference: 'project/extension/long-name', kind: 'extension', scope: 'project',
    name: '自定义工具扩展：用于验证较长名称和首屏布局的扩展',
    facts: {
      installed: true, inUse: true, ownership: 'resource',
      source: 'D:/Projects/rpi-work/.rpi/resources/extension/com.example.long-source-directory/entry.json',
      version: '2.0.0', recordedDigest: 'b'.repeat(64),
    },
  },
  {
    reference: 'global/package/unknown-state', kind: 'package', scope: 'global',
    name: '节点状态未知的全局资源',
    facts: {
      installed: null, inUse: true, ownership: 'resource',
      source: 'D:/Shared/rpi/resources/unknown/entry.json',
      version: '9.9.9', recordedDigest: null,
    },
  },
  {
    reference: 'project/skill/reviewer', kind: 'skill', scope: 'project',
    name: '中文代码审查技能',
    facts: {
      installed: false, inUse: false, ownership: 'resource',
      source: 'D:/Projects/rpi-work/.rpi/resources/skill/reviewer/SKILL.md',
      version: '0.8.1', recordedDigest: 'c'.repeat(64),
    },
  },
];

const discoveredOffers = [
  ['package', '基础工作包', 'audit.offer.base'],
  ['package', '开发工作包', 'audit.offer.dev'],
  ['skill', '代码审查技能', 'audit.offer.skill'],
  ['extension', '浏览器辅助扩展', 'audit.offer.extension'],
].map(([kind, title, id], index) => ({
  manifest: {
    schemaVersion: 1, id, name: id, title, kind, version: `1.${index}.0`,
    description: 'fixture-only typed offer with a long description for resource layout checks',
    author: 'Resource center fixture', origin: 'shipped-with-app', instructions: [],
  },
  sha256: String.fromCharCode(100 + index).repeat(64), fileCount: 4,
  sizeBytes: 4500 + index, acquired: false,
}));

const sessionResources = {
  configuration: {
    revision: 7,
    desired: {
      mainPackage: { reference: nativeResources[0].reference, identity: 'base-tools@1.2.3', diagnostic: null },
      packageInstructions: [],
      skills: [],
      extensions: [{ reference: nativeResources[1].reference, identity: 'long-name@2.0.0', diagnostic: null }],
      tools: { mode: 'default' },
      supplementaryInstructions: '仅来自已保存的会话配置，不是节点加载状态。',
    },
    historicalObservation: null,
  },
  operations: [], readiness: 'verified', executable: true,
  actual: {
    loadedExtensions: ['自定义工具扩展：用于验证较长名称和首屏布局的扩展'],
    effectiveTools: ['read', 'bash'], instructionSources: ['基础工作资源包'],
  },
  generation: 'resource-center-fixture-generation', diagnostic: null,
};

(async () => {
  fs.mkdirSync(outputDir, { recursive: true });
  const results = [];
  const errors = [];
  const geometry = [];
  const mutations = [];
  const requests = [];
  let fixture;
  let browser;
  let page;
  const pass = name => { results.push(name); console.log(`PASS ${name}`); };
  const cardFor = name => page.locator('.native-resource-card').filter({ hasText: name }).first();
  const settingsEntry = () => page.getByRole('button', { name: /会话资源(设置|配置)/ }).first();
  const settingsDialog = () => page.getByRole('dialog', { name: /会话设置|会话资源配置/ }).first();

  try {
    assert(bundleUrl, 'WORK_UI_URL must name a freshly built WASM web bundle; refusing the legacy preview/default');
    assert(!/prototype|target\/parity-web|preview/i.test(bundleUrl), `refusing preview-like WORK_UI_URL: ${bundleUrl}`);

    fixture = await startFixture(0);
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
    page = await browser.newPage({ viewport: { width: 1440, height: 768 } });
    page.setDefaultTimeout(12000);
    page.on('pageerror', error => errors.push(error.message));

    let catalogFailed = false;
    await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
    await page.route('**/api/v1/projects/*/native-resources', async route => {
      const request = route.request();
      requests.push({ method: request.method(), path: new URL(request.url()).pathname });
      if (request.method() === 'GET') {
        return route.fulfill({ status: catalogFailed ? 503 : 200, json: catalogFailed
          ? { error: 'Resource center fixture: catalog unavailable' }
          : { profile: { install: true, inspect: true, remove: true, update: true }, resources: nativeResources, diagnostics: [] } });
      }
      let body = {};
      try { body = request.postDataJSON() || {}; } catch { /* malformed body is recorded below */ }
      mutations.push({ method: request.method(), path: new URL(request.url()).pathname, body });
      return route.fulfill({ status: 200, json: { success: true, observation: { operation: body.operation, reference: body.reference }, diagnostics: 'Fixture receipt only; no actual native operation executed.' } });
    });
    await page.route('**/api/v1/projects/*/packages', async route => {
      requests.push({ method: route.request().method(), path: new URL(route.request().url()).pathname });
      return route.fulfill({ status: 200, json: { source: '随应用发布的只读测试目录', packages: discoveredOffers, diagnostics: [] } });
    });
    await page.route('**/api/v1/sessions/*/resources', async route => {
      requests.push({ method: route.request().method(), path: new URL(route.request().url()).pathname });
      return route.fulfill({ status: 200, json: sessionResources });
    });

    await page.addInitScript(address => {
      localStorage.setItem('rpi-work-nodes-v1', JSON.stringify({ version: 1, recent: [address], selected: address }));
      localStorage.setItem('rpi-work-appearance-v1', JSON.stringify({ theme: 'codex', mode: 'system', density: 'standard' }));
    }, fixture.url);
    await page.goto(bundleUrl);
    await page.locator('[data-history-id="user-dark"]').waitFor();
    const draft = page.locator('.composer textarea');
    const draftText = '资源设置必须保留的会话草稿';
    await draft.fill(draftText);

    // Nothing is fetched/rendered into a resource view before an explicit sidebar visit.
    assert.equal(await page.locator('.package-discovery').count(), 0, 'discovery is lazy before first visit');
    assert.equal(await page.locator('.resources-page').count(), 0, 'legacy resource wrapper is lazy before first visit');
    const nav = page.locator('.primary-nav');
    for (const name of ['扩展', 'Packages', '技能库']) await nav.getByRole('button', { name, exact: true }).waitFor();
    pass('sidebar exposes the three resource categories and keeps resource wrappers lazy');

    await nav.getByRole('button', { name: '扩展', exact: true }).click();
    const center = page.locator('.resource-center');
    await center.waitFor();
    const viewNav = center.getByRole('navigation', { name: '资源视图', exact: true });
    for (const name of ['当前节点', '发现资源', '本地设计']) {
      const button = viewNav.getByRole('button', { name, exact: true });
      await button.waitFor();
      assert.equal(await button.getAttribute('aria-pressed'), name === '当前节点' ? 'true' : 'false');
      assert.equal(await button.getAttribute('aria-current'), name === '当前节点' ? 'page' : 'false');
    }
    assert(await center.getByLabel('搜索节点资源', { exact: true }).count());
    const range = center.getByLabel('资源范围', { exact: true });
    assert.deepEqual(await range.locator('option').evaluateAll(options => options.map(option => option.value)), ['all', 'project', 'global']);
    await cardFor(nativeResources[1].name).waitFor();
    assert.notEqual(await center.getAttribute('aria-busy'), 'true');
    assert.equal(await center.locator('.native-resource-card [role="status"]').count(), 3);
    assert.equal(await center.locator('.native-load-state[role="status"]').first().getAttribute('aria-label'), '实际加载状态：未核实');
    assert.equal(await center.locator('.native-resource-card h3').allTextContents().then(names => names.some(name => name.includes(' · '))), false);
    const extensionCard = cardFor(nativeResources[1].name);
    assert.doesNotMatch(await extensionCard.locator('.native-install-state').innerText(), /已加载|运行中|inUse/);
    assert.match(await extensionCard.locator('.native-selection-state').innerText(), /会话|已保存|选择/);
    assert.equal(mutations.length, 0, 'reading native state never mutates the node');
    pass('native cards expose installed facts and saved selection only; null installed/inUse is not treated as loaded');

    // Every category displays only its own native cards and discovered offers.
    for (const [category, kind, nativeNames, offerTitle] of [
      ['扩展', 'extension', [nativeResources[1].name], '浏览器辅助扩展'],
      ['Packages', 'package', [nativeResources[0].name, nativeResources[2].name], '基础工作包'],
      ['技能库', 'skill', [nativeResources[3].name], '代码审查技能'],
    ]) {
      await nav.getByRole('button', { name: category, exact: true }).click();
      const current = page.locator('.resource-center');
      await current.locator('.native-resource-card').first().waitFor();
      const nativeText = await current.locator('.native-resource-card').allTextContents();
      for (const text of nativeText) assert(nativeNames.some(name => text.includes(name)), `${category} leaked a native resource: ${text}`);
      assert.equal(nativeText.length, nativeNames.length, 'exact category count');
      if (kind === 'package') {
        const unknownCard = cardFor(nativeResources[2].name);
        assert.match(await unknownCard.locator('.native-install-state').innerText(), /状态未核实/);
        assert.doesNotMatch(await unknownCard.innerText(), /已加载|正在运行|inUse/);
      }
      if (kind === 'package') {
        const search = current.getByLabel('搜索节点资源', {exact: true});
        const scope = current.getByLabel('资源范围', {exact: true});
        await scope.selectOption('global');
        assert.deepEqual(await current.locator('.native-resource-card h3').allTextContents(), [nativeResources[2].name]);
        await search.fill('没有这条资源');
        await current.getByText('没有匹配的节点资源，请调整搜索或范围。', {exact: true}).waitFor();
        assert.equal(await current.locator('.native-resource-card').count(), 0);
        await scope.selectOption('all');
        await search.fill('BASE-TOOLS');
        await cardFor(nativeResources[0].name).waitFor();
        assert.equal(await current.locator('.native-resource-card').count(), 1);
        const searchValue = await search.inputValue();
        const views = current.getByRole('navigation', {name: '资源视图'});
        await views.getByRole('button', {name: '发现资源', exact: true}).click();
        await current.locator('.package-discovery .package-card').first().waitFor();
        await views.getByRole('button', {name: '当前节点', exact: true}).click();
        assert.equal(await search.inputValue(), searchValue);
        await search.fill('');
        pass('native search and scope intersect; visited view changes preserve the node search');
      }
      await current.getByRole('navigation', {name: '资源视图'}).getByRole('button', {name: '发现资源', exact: true}).click();
      await current.locator('.package-discovery .package-card').first().waitFor();
      const offerText = await current.locator('.package-discovery .package-card').allTextContents();
      assert(offerText.some(text => text.includes(offerTitle)), `${category} lacks its typed discovery offer`);
      const expected = discoveredOffers.filter(o => o.manifest.kind === kind).map(o => o.manifest.title);
      assert.equal(offerText.length, expected.length);
      assert(offerText.every(t => expected.some(name => t.includes(name))), `${category}: discovery category leak`);
      await current.getByRole('navigation', {name: '资源视图'}).getByRole('button', {name: '本地设计', exact: true}).click();
      await current.locator('.resources-page').waitFor();
    }
    assert.equal(await page.locator('.package-discovery').count(), 1, 'visited discovery wrapper is retained');
    assert.equal(await page.locator('.resources-page').count(), 1, 'visited legacy/design wrapper is retained');
    pass('category navigation restricts native and discovered resources while retaining visited hidden views');

    await nav.getByRole('button', { name: '扩展', exact: true }).click();
    const native = cardFor(nativeResources[1].name);
    const detailButton = native.getByRole('button', { name: '查看详情', exact: true });
    await detailButton.click();
    const detail = page.getByRole('dialog', { name: '资源详情', exact: true });
    await detail.waitFor();
    assert.equal(await page.locator('dialog:modal').count(), 1, 'native details use an HTML modal dialog');
    assert(await detail.locator('.native-detail-body').count());
    assert.equal(await detail.getByText('实际加载', { exact: true }).count(), 1);
    assert.match(await detail.innerText(), /实际加载：?\s*未核实/);
    await detail.evaluate(async element => { await Promise.all(element.getAnimations().map(animation => animation.finished)); });
    const detailBounds = await detail.boundingBox();
    assert(detailBounds.width <= 700 && detailBounds.width <= 1440 && detailBounds.height <= 768);
    await page.setViewportSize({ width: 390, height: 844 });
    const mobileDetailBounds = await detail.boundingBox();
    assert(mobileDetailBounds.x >= -1 && mobileDetailBounds.y >= -1 && mobileDetailBounds.x + mobileDetailBounds.width <= 391 && mobileDetailBounds.y + mobileDetailBounds.height <= 845, JSON.stringify(mobileDetailBounds));
    await page.setViewportSize({ width: 1440, height: 768 });
    const close = detail.getByRole('button', { name: '关闭弹框', exact: true });
    await page.keyboard.press('Escape');
    await detail.waitFor({ state: 'hidden' });
    assert(await detailButton.evaluate(element => element === document.activeElement), 'Escape returns focus to 查看详情');
    await detailButton.click();
    await detail.waitFor();
    await close.click();
    await detail.waitFor({ state: 'hidden' });
    await detailButton.click();
    await detail.waitFor();
    const management = detail.locator('summary').filter({ hasText: '管理此资源' });
    await management.click();
    for (const label of ['更新来源', '重新安装', '移除资源']) await detail.getByText(label, { exact: true }).waitFor();
    await detail.getByRole('button', {name: '更新来源', exact: true}).click();
    await detail.getByLabel('此资源的节点来源', { exact: true }).waitFor();
    const source = detail.getByLabel('此资源的节点来源', { exact: true });
    const version = detail.getByLabel('此资源的记录版本', { exact: true });
    const trusted = detail.getByRole('checkbox', { name: /我已核对此资源和来源/ });
    const submitUpdate = detail.getByRole('button', { name: '确认并提交', exact: true });
    assert(await submitUpdate.isDisabled());
    await trusted.check();
    assert(await submitUpdate.isDisabled(), 'source is required even when trust is checked');
    await source.fill('D:/Projects/rpi-work/.rpi/resources/extension/updated-source/entry.json');
    assert.equal(await trusted.isChecked(), false, 'changing source resets authorization');
    await version.fill('2.1.0');
    assert(await submitUpdate.isDisabled());
    await trusted.check();
    await version.fill('2.1.1');
    assert.equal(await trusted.isChecked(), false, 'changing the recorded version also resets authorization');
    await trusted.check();
    assert(await submitUpdate.isEnabled());
    await submitUpdate.click();
    await page.waitForTimeout(100);
    assert.equal(mutations.length, 1, 'update is the first node mutation');
    assert.equal(mutations[0].body.operation, 'update');
    assert.equal(mutations[0].body.reference, nativeResources[1].reference);
    assert.equal(mutations[0].body.version, '2.1.1');
    assert.equal(mutations[0].body.trusted, true);
    await page.keyboard.press('Escape');
    await detail.waitFor({ state: 'hidden' });
    await native.getByRole('button', { name: '查看详情', exact: true }).click();
    const removeSummary = page.getByRole('dialog', { name: '资源详情', exact: true }).locator('summary').filter({ hasText: '管理此资源' });
    await removeSummary.click();
    const remove = page.getByRole('dialog', { name: '资源详情', exact: true }).getByRole('button', { name: '移除资源', exact: true });
    await remove.click();
    const confirmation = page.getByRole('region', { name: '移除确认', exact: true });
    await confirmation.waitFor();
    assert.equal(mutations.length, 1, 'opening removal confirmation is not a mutation');
    await confirmation.getByRole('button', { name: '取消', exact: true }).click();
    assert.equal(mutations.length, 1);
    await remove.click();
    await page.getByRole('region', { name: '移除确认', exact: true }).getByRole('button', { name: '确认移除', exact: true }).click();
    await page.waitForTimeout(100);
    assert.equal(mutations.length, 2);
    assert.equal(mutations[1].body.operation, 'remove');
    assert.equal(mutations[1].body.reference, nativeResources[1].reference);
    await page.keyboard.press('Escape');
    await detail.waitFor({state: 'hidden'});
    pass('details are bounded/keyboard-closable and management mutates only after explicit trusted update or removal confirmation');

    catalogFailed = true;
    await nav.getByRole('button', { name: '扩展', exact: true }).click();
    await center.getByRole('button', { name: '刷新目录 / 回读', exact: true }).click();
    await center.getByRole('alert').filter({ hasText: '目录读取失败' }).waitFor();
    assert.equal((await center.innerText()).includes('正在读取节点目录…'), false);
    assert.match(await center.getByRole('alert').innerText(), /目录读取失败/);
    pass('native catalog failure reports a retryable error without stale loading text');

    catalogFailed = false;
    await center.getByRole('button', { name: '刷新目录 / 回读', exact: true }).click();
    await cardFor(nativeResources[1].name).waitFor();
    await nav.getByRole('button', { name: 'Packages', exact: true }).click();
    await settingsEntry().click();
    const settings = settingsDialog();
    await settings.waitFor();
    const designTab = settings.getByRole('tab', { name: /设计引用|本地设计/ }).first();
    if (await designTab.count()) await designTab.click();
    assert.equal(await settings.locator('.package-discovery').count(), 0, 'session settings has no discovery catalog');
    assert.equal(await settings.locator('.native-resource-card').count(), 0, 'session settings has no node management list');
    assert.match(await settings.innerText(), /configuration_only|会话|设计|资源/);
    await page.keyboard.press('Escape');
    await settings.waitFor({ state: 'hidden' });
    assert.equal(await draft.inputValue(), draftText);
    pass('session resource settings reuse the root session design tab and preserve the composer draft');

    // Capture both themes and all requested widths. The extension card is intentionally
    // long and must remain visible on the first screen without a horizontal strip.
    await nav.getByRole('button', { name: '扩展', exact: true }).click();
    for (const mode of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: mode });
      for (const width of [1440, 768, 390, 320]) {
        const height = width < 600 ? 844 : 768;
        await page.setViewportSize({ width, height });
        await page.waitForTimeout(80);
        const metrics = await center.evaluate(root => {
          const visible = element => element && element.checkVisibility({ checkVisibilityCSS: true, checkOpacity: true });
          const box = element => { const r = element.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
          const extension = [...root.querySelectorAll('.native-resource-card')].find(card => card.textContent.includes('自定义工具扩展'));
          return {
            viewport: { width: innerWidth, height: innerHeight },
            center: box(root), scrollWidth: root.scrollWidth, clientWidth: root.clientWidth,
            extension: extension ? box(extension) : null,
            firstScreen: extension ? extension.getBoundingClientRect().bottom <= innerHeight + 1 : false,
            cardHeights: [...root.querySelectorAll('.native-resource-card')].filter(visible).map(box).map(item => item.height),
            controls: [...root.querySelectorAll('button, summary, input:not([type=checkbox]), select')].filter(visible).map(element => ({text: element.textContent || element.getAttribute('aria-label'), ...box(element)})),
          };
        });
        assert(metrics.center.x >= -1 && metrics.center.x + metrics.center.width <= width + 1);
        assert(metrics.scrollWidth <= metrics.clientWidth + 1, `${mode}/${width}: horizontal overflow`);
        assert(metrics.extension && metrics.firstScreen, `${mode}/${width}: extension is not visible in first screen`);
        assert(metrics.cardHeights.every(heightValue => heightValue <= 260), `${mode}/${width}: normal native card exceeds 260px`);
        assert(metrics.controls.every(control => control.height >= 43.9), `${mode}/${width}: control below 44px ${JSON.stringify(metrics.controls.filter(control => control.height < 43.9))}`);
        geometry.push({ mode, width, ...metrics });
        await page.screenshot({ path: path.join(outputDir, `resource-center-${mode}-${width}.png`), fullPage: false, animations: 'disabled' });
        if (width < 600) {
          const button = cardFor(nativeResources[1].name).getByRole('button', {name: '查看详情', exact: true});
          const scrollBefore = await page.locator('.page-view').evaluate(element => element.scrollTop);
          await button.click();
          const mobileDetail = page.getByRole('dialog', {name: '资源详情', exact: true});
          await mobileDetail.waitFor();
          await mobileDetail.evaluate(async element => { await Promise.all(element.getAnimations().map(animation => animation.finished)); });
          await page.screenshot({path: path.join(outputDir, `resource-detail-${mode}-${width}.png`), animations: 'disabled'});
          await page.setViewportSize({width, height: 500});
          const reduced = await mobileDetail.evaluate(element => {
            const r = element.getBoundingClientRect(), body = element.querySelector('.native-detail-body');
            return {x:r.x,y:r.y,width:r.width,height:r.height,client:body.clientHeight,scroll:body.scrollHeight,overflow:getComputedStyle(body).overflowY};
          });
          assert(reduced.y >= -1 && reduced.y + reduced.height <= 501);
          assert.equal(reduced.overflow, 'auto');
          assert(reduced.scroll > reduced.client, 'reduced-height mobile detail has an internal scroll area');
          await page.setViewportSize({width, height});
          await page.keyboard.press('Escape');
          await mobileDetail.waitFor({state:'hidden'});
          assert(await button.evaluate(element => element === document.activeElement));
          assert(Math.abs((await page.locator('.page-view').evaluate(element => element.scrollTop)) - scrollBefore) <= 1);
        }
      }
    }
    pass('light/dark screenshots at 1440/768/390/320 have no horizontal overflow, 44px controls, bounded cards and visible extension');
    assert.deepEqual(errors, [], errors.join('\n'));
  } catch (error) {
    results.push(`FAILED: ${error.message}`);
    if (page) await page.screenshot({ path: path.join(outputDir, 'resource-center-failure.png'), animations: 'disabled' }).catch(() => {});
    console.error(error.stack || error);
    process.exitCode = 1;
  } finally {
    fs.writeFileSync(reportPath, JSON.stringify({
      status: process.exitCode ? 'failed' : 'passed', bundleUrl, results, errors, geometry, requests, mutations,
      assumptions: ['WORK_UI_URL is supplied by the parent after a fresh WASM build.', 'Node catalog, session resources and package discovery are typed browser fixtures; no model or real node is contacted.'],
      generatedAtUtc: new Date().toISOString(),
    }, null, 2));
    if (browser) await browser.close().catch(error => errors.push(error.message));
    if (fixture) await fixture.close().catch(error => errors.push(error.message));
  }
})().catch(error => { console.error(error.stack || error); process.exitCode = 1; });
