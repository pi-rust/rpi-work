const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');

(async () => {
  const node = await startFixture();
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.route('https://api.github.com/**', route => route.fulfill({ status: 404, body: '{}' }));
  const page = await context.newPage(), results = [], errors = [];
  page.setDefaultTimeout(10000);
  page.on('pageerror', error => errors.push(error.message));
  const pass = name => { results.push(name); console.log('PASS', name); };
  const creates = () => node.requests.filter(r => r.method === 'POST' && r.path === '/api/v1/sessions');
  const row = id => page.locator(`.project-entry[data-project-id="${id}"]`);
  const composer = () => page.getByRole('textbox', { name: '发送给 agent 的消息', exact: true });
  const title = text => page.locator('.app-header').getByRole('heading', { name: text, exact: true });
  const ready = async () => {
    await page.waitForFunction(() => document.querySelector('.app-header h1')?.textContent === '新会话');
    await page.waitForFunction(() => document.activeElement?.matches('.composer textarea'));
    assert.equal(await composer().inputValue(), '');
    assert.equal(await page.getByRole('dialog', { name: '新建任务', exact: true }).count(), 0);
    assert.equal(await page.getByLabel('任务标题', { exact: true }).count(), 0);
  };
  const waitCount = async (predicate, message) => {
    const until = Date.now() + 10000;
    while (!predicate() && Date.now() < until) await new Promise(resolve => setTimeout(resolve, 25));
    assert(predicate(), message);
  };
  try {
    await page.goto(process.env.WORK_UI_URL || 'http://127.0.0.1:4180/');
    await page.getByRole('button', { name: '打开设置', exact: true }).click();
    await page.getByRole('navigation', { name: '设置分类' }).getByRole('button', { name: '执行节点', exact: true }).click();
    await page.getByRole('button', { name: '连接其他节点', exact: true }).click();
    await page.getByLabel('节点地址', { exact: true }).fill(node.url);
    await page.getByRole('button', { name: '连接节点', exact: true }).click();
    await page.locator('.server-details [role=status]').filter({ hasText: '在线' }).waitFor();
    await page.getByRole('button', { name: '返回会话', exact: true }).click();
    await row('rpi-work').locator('.project-row').click();
    await row('rpi-work').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await title('为工作台添加深色模式').waitFor();
    await composer().fill('原会话未发送的草稿');

    node.controls.sessionDelayMs = 300;
    await row('pi-rust').getByRole('button', { name: '在 pi-rust 中新建会话', exact: true }).dblclick();
    await ready();
    assert.equal(creates().length, 1);
    assert.equal(creates()[0].body.projectId, 'pi-rust');
    assert.equal(node.requests.filter(r => r.path.endsWith('/rename')).length, 0);
    pass('project + opens a focused blank conversation in that project with one POST and no title dialog');
    node.controls.sessionDelayMs = 0;
    await row('rpi-work').locator('.project-row').click();
    await row('rpi-work').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await title('为工作台添加深色模式').waitFor();
    assert.equal(await composer().inputValue(), '原会话未发送的草稿');
    await row('pi-rust').locator('.project-row').click();
    await row('pi-rust').getByRole('button', { name: '新会话', exact: true }).click();
    pass('starting a conversation in another project preserves the original session draft');

    node.controls.failRenameCount = 1;
    await composer().fill('修复项目中的登录问题\n详细说明');
    await page.getByRole('button', { name: '发送消息', exact: true }).click();
    await waitCount(() => node.requests.filter(r => r.path.endsWith('/rename')).length === 1, 'first title request missing');
    await page.waitForFunction(() => document.querySelector('.composer textarea')?.value === '');
    const firstId=(await page.locator('.transcript').getAttribute('data-session-scope')).split('::session::').at(-1);
    node.emit({type:'message_end',message:{entryId:'first-new-message',role:'user',content:'修复项目中的登录问题\n详细说明'}},firstId);
    node.emit({type:'agent_settled',outcome:'completed'},firstId);
    await page.getByRole('button',{name:'发送消息',exact:true}).waitFor();
    await composer().fill('继续检查测试');
    await page.getByRole('button', { name: '发送消息', exact: true }).click();
    await title('修复项目中的登录问题').waitFor();
    assert.equal(creates().length, 1);
    assert.equal(node.requests.filter(r => r.body.type === 'prompt').length, 2);
    assert.equal(node.requests.filter(r => r.path.endsWith('/rename')).at(-1).body.title, '修复项目中的登录问题');
    pass('first accepted message supplies the title; a naming failure never duplicates a session or prompt');

    node.controls.failSession = true;
    await row('rpi-work').locator('.project-new-session').click();
    await page.locator('.empty-state [role=alert]').filter({ hasText: '创建会话失败' }).waitFor();
    node.controls.failSession = false;
    await page.getByRole('button', { name: '开始新任务', exact: true }).click();
    await ready();
    assert.equal(creates().length, 3);
    assert.equal(creates().at(-1).body.projectId, 'rpi-work');
    pass('a failed creation stays in the chosen project and can be retried inline');

    node.controls.sessionDelayMs = 500;
    await row('pi-rust').locator('.project-new-session').click();
    await waitCount(() => creates().length === 4, 'delayed creation did not start');
    await row('rpi-work').locator('.project-row').click();
    await row('rpi-work').getByRole('button', { name: '为工作台添加深色模式', exact: true }).click();
    await page.waitForFunction(() => !document.querySelector('.new-task')?.disabled);
    await title('为工作台添加深色模式').waitFor();
    node.controls.sessionDelayMs = 0;
    pass('a late creation result does not steal focus after the user switches projects');

    await page.getByRole('button', { name: '新建任务', exact: true }).click();
    await ready();
    assert.equal(creates().at(-1).body.projectId, 'rpi-work');
    await composer().fill('/new');
    await composer().press('Enter');
    await waitCount(() => creates().length === 6, '/new did not create a session');
    await ready();
    assert.equal(creates().at(-1).body.projectId, 'rpi-work');
    pass('global New task and /new inherit the current project and open directly');

    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
      const plus = row('pi-rust').locator('.project-new-session');
      const box = await plus.boundingBox();
      assert(Math.round(box.width) >= 44 && Math.round(box.height) >= 44, JSON.stringify(box));
      await page.screenshot({ path: `artifacts/new-conversation-projects-${width}.png`, animations: 'disabled' });
      await plus.click();
      await ready();
      assert.equal(creates().at(-1).body.projectId, 'pi-rust');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.screenshot({ path: `artifacts/new-conversation-${width}.png`, animations: 'disabled' });
    }
    pass('390/320 layouts keep project + touch targets accessible and close navigation for the new conversation');
    assert.deepEqual(errors, []);
    pass('no application JavaScript errors');
  } catch (error) {
    errors.push(error.message);
    await page.screenshot({ path: 'artifacts/new-conversation-failure.png', animations: 'disabled' });
    throw error;
  } finally {
    fs.writeFileSync('artifacts/new-conversation-validation.json', JSON.stringify({ results, errors }, null, 2));
    await browser.close();
    await node.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
