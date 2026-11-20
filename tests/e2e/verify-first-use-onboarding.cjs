// Real WASM + isolated node. Configure a provider, open a project and send the
// first fixture message through the same entries available to a first-time user.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const { bridgeRealtimePolicies } = require('../support/realtime-http-policy-bridge.cjs');

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture();
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.setDefaultTimeout(12000);
  await bridgeRealtimePolicies(page, () => [new URL(fixture.url).origin]);
  const errors = [], results = [], providerWrites = [];
  const prompts = () => fixture.requests.filter(request => request.body?.type === 'prompt');
  let catalog = { revision: 'onboarding-0', providers: [] };
  let failCatalog = false, hasProject = false;
  const button = (name, scope = page) => scope.getByRole('button', { name, exact: true });
  const home = page.locator('.home-content');
  const guide = page.getByRole('region', { name: '首次使用引导', exact: true });
  const input = home.locator('textarea');
  const pass = message => { results.push(message); console.log('PASS', message); };
  page.on('pageerror', error => errors.push(error.message));
  await page.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  await page.route('**/api/v1/projects/*/native-resource-config', route =>
    route.fulfill({ status: 501, json: { error: 'Unsupported by fixture' } }));
  await page.route(fixture.url + 'api/v1/providers', async route => {
    if (route.request().method() === 'GET') {
      return failCatalog ? route.fulfill({ status: 503, json: { error: 'Fixture unavailable' } })
        : route.fulfill({ json: catalog });
    }
    const { expectedRevision, action } = route.request().postDataJSON();
    assert.equal(expectedRevision, catalog.revision);
    providerWrites.push(action.type);
    if (action.type === 'upsert') {
      catalog.providers = [{
        id: action.definition.id, configured: true, configKeyPresent: false,
        storedCredential: 'none', diagnostic: null, definition: action.definition,
      }];
    } else if (action.type === 'setKey') {
      catalog.providers[0].storedCredential = 'apiKey';
    } else {
      throw new Error(`Unexpected provider mutation ${action.type}`);
    }
    catalog.revision = `onboarding-${providerWrites.length}`;
    return route.fulfill({ json: { catalog, requiresRestart: false } });
  });
  await page.route(fixture.url + 'api/v1/projects', async route => {
    if (route.request().method() === 'POST') {
      const response = await route.fetch();
      if (response.ok()) hasProject = true;
      return route.fulfill({ response });
    }
    const response = await route.fetch();
    const body = await response.json();
    body.projects = hasProject ? body.projects.filter(project => project.name === 'onboarding') : [];
    return route.fulfill({ response, json: body });
  });
  await page.route(fixture.url + 'api/v1/sessions', async route => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch(), body = await response.json();
    body.sessions = body.sessions.filter(session => session.sessionId.startsWith('new-'));
    return route.fulfill({ response, json: body });
  });
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:14180/');
    await input.fill('首次使用时保留的想法');
    await button('打开设置').click();
    await button('工作协同', page.getByRole('navigation', { name: '设置分类' })).click();
    await button('输入地址', page.locator('.collaboration-hub .connect-card')).click();
    await page.getByLabel('节点地址', { exact: true }).fill(fixture.url);
    await button('连接节点').click();
    await page.locator('.collaboration-page-heading [role=status]').filter({ hasText: '已连接' }).waitFor();
    await button('返回会话').click();
    await guide.getByRole('heading', { name: '配置模型，开始使用', exact: true }).waitFor();
    await input.fill('配置过程中保留这条草稿');
    assert(await button('发送消息', home).isDisabled());
    for (const [width, height] of [[1440, 900], [390, 844], [320, 420]]) {
      await page.setViewportSize({ width, height });
      await page.waitForFunction(width => document.querySelector('.app-shell').dataset.deviceMode === (width < 768 ? 'mobile' : 'desktop'), width);
      await page.evaluate(async () => {
        await Promise.all(document.getAnimations()
          .filter(animation => Number.isFinite(animation.effect.getTiming().iterations))
          .map(animation => animation.finished.catch(() => {})));
      });
      await guide.scrollIntoViewIfNeeded();
      assert(await guide.evaluate(element => {
        const rect = element.getBoundingClientRect();
        return rect.width <= innerWidth && element.scrollWidth <= element.clientWidth + 1;
      }));
      await page.screenshot({ path: `artifacts/onboarding-model-${width}.png` });
    }
    pass('missing model configuration opens actionable onboarding at desktop, phone and short-screen sizes');
    await page.setViewportSize({ width: 1440, height: 900 });
    await button('配置模型', guide).click();
    const providers = page.getByRole('region', { name: '节点供应商配置', exact: true });
    await button('添加供应商', providers).click();
    const definition = page.locator('#provider-definition');
    await definition.getByLabel('供应商 ID', { exact: true }).fill('deepseek');
    await definition.getByLabel('显示名称', { exact: true }).fill('Onboarding fixture');
    await definition.getByLabel('协议', { exact: true }).selectOption('openai-completions');
    await definition.getByLabel('Base URL', { exact: true }).fill('https://fixture.invalid/v1');
    await definition.getByLabel('模型 ID', { exact: true }).fill('deepseek-v4-pro');
    await definition.getByLabel('模型名称', { exact: true }).fill('Onboarding model');
    await button('仅保存定义', definition).click();
    await providers.getByRole('status').filter({ hasText: '已权威回读' }).waitFor();
    await button('保存独立密钥', providers).click();
    await page.getByLabel('新 API Key', { exact: true }).fill('isolated-onboarding-fixture-key');
    await button('仅保存 API Key', page.getByRole('dialog')).click();
    await providers.getByRole('status').filter({ hasText: '已权威回读' }).waitFor();
    await button('返回会话').click();
    await guide.getByRole('heading', { name: '选择一个工作项目', exact: true }).waitFor();
    assert.equal(await input.inputValue(), '配置过程中保留这条草稿');
    assert.deepEqual(providerWrites, ['upsert', 'setKey']);
    pass('existing provider and credential forms advance onboarding after authoritative readback without losing drafts');

    await button('打开项目目录', guide).click();
    const project = page.locator('#project-picker');
    await project.getByLabel('已有项目目录', { exact: true }).fill('D:/Projects/onboarding');
    await button('打开项目', project).click();
    await project.waitFor({ state: 'hidden' });
    await home.getByText('onboarding', { exact: true }).first().waitFor();
    await guide.waitFor({ state: 'hidden' });
    assert.equal(await input.inputValue(), '配置过程中保留这条草稿');
    await page.waitForFunction(() => !document.querySelector('.home-send')?.disabled);
    assert.equal(prompts().length, 0);
    await input.press('Enter');
    await page.waitForFunction(() => !document.querySelector('.home-content'));
    const deadline = Date.now() + 10000;
    while (prompts().length === 0 && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    const prompt = prompts();
    assert.equal(prompt.length, 1);
    assert.match(JSON.stringify(prompt[0].body), /配置过程中保留这条草稿/);
    pass('opening a project completes onboarding and the first Enter creates a session and submits one real fixture prompt');

    failCatalog = true;
    await page.locator('.home-nav').click();
    await guide.getByRole('heading', { name: '暂时无法确认模型配置', exact: true }).waitFor();
    await input.fill('配置检查失败仍保留草稿');
    failCatalog = false;
    await button('重新检查配置', guide).click();
    await guide.waitFor({ state: 'hidden' });
    assert.equal(await input.inputValue(), '配置检查失败仍保留草稿');
    await page.reload();
    await home.waitFor();
    await page.waitForFunction(() => document.querySelector('.home-project-name')?.textContent === 'onboarding');
    await page.waitForFunction(() => document.querySelector('.home-send')?.disabled === false);
    assert.equal(await guide.count(), 0);
    assert.equal(await input.inputValue(), '配置检查失败仍保留草稿');
    assert.deepEqual(errors, []);
    pass('catalog errors recover through retry and a configured workspace reopens without repeating onboarding');
    fs.writeFileSync('artifacts/first-use-onboarding-validation.json', JSON.stringify({ results, errors }, null, 2));
  } catch (error) {
    await page.screenshot({ path: 'artifacts/first-use-onboarding-failure.png' });
    fs.writeFileSync('artifacts/first-use-onboarding-validation.json', JSON.stringify({ results, errors, fatal: error.message }, null, 2));
    throw error;
  } finally {
    await browser.close();
    await fixture.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
