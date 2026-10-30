const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');

const KEY = 'rpi-work-resources-v1';
function design(id, name, agent, ui, version = '1.0.0') {
  const manifest = { schemaVersion: 1, kind: 'rpi-work-extension-design', id, name, version };
  if (agent) manifest.agent = { runtime: 'rpi-native', entry: 'src/lib.rs', tools: ['review'], permissions: ['project.read'] };
  if (ui) manifest.ui = { schemaVersion: 1, contributions: [{ id: 'status', slot: 'session-inspector', title: 'Review status', widgets: [
    { type: 'text', text: '<img src=x onerror="window.extensionExecuted=true">' },
    { type: 'fields', fields: [{ label: 'Scope', value: 'Static design' }] },
    { type: 'progress', label: 'Review', value: 2, max: 10 },
  ] }] };
  return { schemaVersion: 1, kind: 'rpi-work-design-package', id, name, version, author: 'QA', description: 'Extension architecture regression', resources: { skills: [], tools: [], prompts: [] }, design: { resourceKind: 'Extension', sources: { 'extension.design.json': JSON.stringify(manifest), 'src/lib.rs': '// not executable' } }, extraMetadata: { preserve: true } };
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport: { width: 1100, height: 950 } });
  page.setDefaultTimeout(8000);
  const errors = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  const saved = () => page.evaluate(key => JSON.parse(localStorage.getItem(key)), KEY);
  const card = (root, name, version = '1.0.0') => root.locator('.resource-card').filter({ hasText: name }).filter({ hasText: version });
  const config = () => page.getByRole('dialog', { name: '会话资源配置', exact: true });
  const details = () => page.getByRole('dialog', { name: '资源详情', exact: true });
  async function importDesign(document, valid = true) {
    await page.getByRole('button', { name: '导入资源包', exact: true }).click();
    const modal = page.getByRole('dialog', { name: '导入设计包', exact: true });
    await modal.locator('textarea').fill(JSON.stringify(document));
    await modal.getByRole('button', { name: '校验并导入', exact: true }).click();
    if (valid) {
      await modal.waitFor({ state: 'hidden' });
    } else {
      await modal.locator('.resource-error').waitFor();
      await modal.getByRole('button', { name: '取消', exact: true }).click();
    }
  }
  async function openConfig() {
    await page.getByRole('button', { name: '打开会话资源配置', exact: true }).click();
    await config().getByRole('button', { name: '扩展', exact: true }).click();
  }
  async function waitSaved(predicate) {
    await page.waitForFunction(({ key, source }) => {
      const data = JSON.parse(localStorage.getItem(key));
      return data && (new Function('data', `return (${source})(data)`))(data);
    }, { key: KEY, source: predicate.toString() });
  }
  try {
    await page.goto(process.env.EXTENSION_REVIEW_URL || 'http://127.0.0.1:4182/modules/all/');
    await page.getByRole('button', { name: 'extensions', exact: true }).click();
    const combined = design('qa.combined', 'Combined Design', true, true);
    for (const document of [design('qa.agent', 'Agent Design', true, false), design('qa.ui', 'UI Design', false, true), combined]) await importDesign(document);
    assert.match(await card(page, 'Agent Design').innerText(), /Agent 扩展设计/);
    assert.match(await card(page, 'UI Design').innerText(), /纯 UI 扩展设计/);
    assert.equal(await card(page, 'UI Design').getByRole('button', { name: '节点管理暂不支持', exact: true }).count(), 0);
    assert.match(await card(page, 'UI Design').innerText(), /客户端预览/);
    assert.match(await card(page, 'Combined Design').innerText(), /Agent \+ UI 扩展设计/);
    assert.equal(await page.getByRole('button', { name: '已启用', exact: true }).count(), 0);
    const [download] = await Promise.all([page.waitForEvent('download'), card(page, 'Combined Design').getByRole('button', { name: '导出 JSON', exact: true }).click()]);
    assert.deepEqual(JSON.parse(fs.readFileSync(await download.path(), 'utf8')), combined);
    pass('three extension kinds import without running code or claiming node installation; export is immutable');

    const invalid = design('qa.invalid', 'Invalid UI', true, true);
    const manifest = JSON.parse(invalid.design.sources['extension.design.json']);
    manifest.ui.contributions[0].widgets[0] = { type: 'html', html: '<script>bad</script>' };
    invalid.design.sources['extension.design.json'] = JSON.stringify(manifest);
    await importDesign(invalid, false);
    assert.equal(await card(page, 'Invalid UI').count(), 0);
    pass('executable or unknown UI declarations reject the whole import atomically');

    await openConfig();
    assert.equal(await card(config(), 'UI Design').getByRole('button', { name: '选择 Agent 版本', exact: true }).count(), 0);
    await card(config(), 'Combined Design').getByRole('button', { name: '选择 Agent 版本', exact: true }).click();
    await waitSaved(data => data.sessions.some(session => session.extension_pins?.some(pin => pin.id === 'qa.combined' && pin.version === '1.0.0')));
    const before = (await saved()).sessions;
    await card(config(), 'Combined Design').getByRole('button', { name: '查看 Agent / UI', exact: true }).click();
    await details().getByText('<img src=x onerror="window.extensionExecuted=true">', { exact: true }).waitFor();
    assert.equal(await details().locator('.extension-widgets img, .extension-widgets script').count(), 0);
    assert.equal(await page.evaluate(() => window.extensionExecuted), undefined);
    await details().getByRole('button', { name: '隐藏 UI', exact: false }).click();
    await waitSaved(data => data.uiPreferences?.some(pref => pref.visible === false) || data.ui_preferences?.some(pref => pref.visible === false));
    assert.deepEqual((await saved()).sessions, before);
    pass('UI renders escaped data and hiding it changes neither Agent pins nor session revision');
    await page.keyboard.press('Escape');
    if (await config().isVisible()) await page.keyboard.press('Escape');

    await page.getByRole('button', { name: 'Toggle extension lock', exact: true }).click();
    await openConfig();
    assert.equal(await card(config(), 'Combined Design').getByRole('button', { name: '移除 Agent 选择', exact: true }).isDisabled(), true);
    await card(config(), 'Combined Design').getByRole('button', { name: '查看 Agent / UI', exact: true }).click();
    assert.equal(await details().getByRole('button', { name: '显示 UI', exact: false }).isEnabled(), true);
    pass('runtime lock disables Agent configuration but not independent UI preferences');
    await page.keyboard.press('Escape');
    if (await config().isVisible()) await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Toggle extension lock', exact: true }).click();

    await importDesign(design('qa.combined', 'Combined Design', true, true, '2.0.0'));
    assert.equal((await saved()).sessions.find(session => session.session_id === 'review-session-a').extension_pins[0].version, '1.0.0');
    await page.getByRole('button', { name: 'Session B', exact: true }).click();
    await openConfig();
    assert.equal(await config().getByRole('button', { name: '移除 Agent 选择', exact: true }).count(), 0);
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Session A', exact: true }).click();
    await page.reload();
    await page.getByRole('button', { name: 'extensions', exact: true }).click();
    await openConfig();
    await card(config(), 'Combined Design', '1.0.0').getByRole('button', { name: '移除 Agent 选择', exact: true }).waitFor();
    await card(config(), 'Combined Design', '1.0.0').getByRole('button', { name: '查看 Agent / UI', exact: true }).click();
    await details().getByRole('button', { name: '显示 UI', exact: false }).waitFor();
    pass('fixed versions, session isolation and hidden UI preference survive new imports and reload');
    await page.keyboard.press('Escape');
    if (await config().isVisible()) await page.keyboard.press('Escape');

    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await openConfig();
      assert(await config().evaluate(element => element.scrollWidth <= element.clientWidth + 1));
      await page.keyboard.press('Escape');
    }
    pass('resource page and extension configuration remain bounded at 390 and 320 pixels');
    assert.deepEqual(errors, []);
    pass('no application JavaScript errors');
  } finally {
    fs.mkdirSync('artifacts', { recursive: true });
    fs.writeFileSync('artifacts/extensions-module-validation.json', JSON.stringify({ scope: 'test-only Dioxus module harness, no live node plugin protocol', results, errors }, null, 2));
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
