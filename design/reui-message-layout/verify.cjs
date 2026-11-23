const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { checkHistoryViews } = require('./verify-history.cjs');
const { checkComposerCaret } = require('./verify-composer.cjs');
const { checkReadingRestoration } = require('./verify-reading.cjs');
const { checkTurnHierarchy } = require('./verify-hierarchy.cjs');
const playwright = process.env.RPI_LAYOUT_PLAYWRIGHT ?? '../../tools/node_modules/playwright';
const { chromium } = require(playwright);
const url = pathToFileURL(path.join(__dirname, 'index.html')).href;

async function checkViewport(page) {
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  const [transcript, composer] = await Promise.all([
    page.locator('#transcript').boundingBox(),
    page.locator('.composer').boundingBox(),
  ]);
  assert.equal(transcript.x, composer.x);
  assert.equal(transcript.width, composer.width);
  assert.ok(composer.y + composer.height <= page.viewportSize().height);
}

async function checkCodexFlows(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const errors = [];
  const remoteRequests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (!request.url().startsWith('file:')) remoteRequests.push(request.url());
  });
  try {
    await page.clock.install();
    await page.goto(url);
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    const button = (name, root = page) => root.getByRole('button', { name, exact: true });
    const draft = page.getByRole('textbox', { name: '消息', exact: true });
    const review = page.getByRole('complementary', { name: '工作区面板' });
    const send = async text => {
      await draft.fill(text);
      await draft.press('Enter');
    };
    const scenario = async value => {
      await button('设置').click();
      await button('常规').click();
      await page.getByLabel('下一轮演示').selectOption(value);
      await button('关闭设置').click();
    };

    await button('暂存', review).click();
    assert.equal(await review.getByText('已暂存', { exact: true }).isVisible(), true);
    await button('取消暂存', review).click();
    assert.equal(await review.getByText('已暂存', { exact: true }).count(), 0);
    await button('在 src/components/transcript.tsx:83 添加反馈', review).focus();
    await page.keyboard.press('Enter');
    await page.locator('#review-comment').fill('补测空列表');
    await button('加入下一条消息').click();
    assert.equal(await review.getByText('补测空列表', { exact: true }).isVisible(), true);
    await send('处理行内反馈');
    assert.match(await page.locator('.demo-turn .prompt').textContent(), /transcript\.tsx:83 · 补测空列表/);
    await page.clock.runFor(1800);

    await button('运行审阅', review).click();
    await page.clock.runFor(1800);
    assert.equal(await review.getByText('检查空会话', { exact: true }).isVisible(), true);
    await button('关闭发现', review).click();
    assert.equal(await review.getByText('检查空会话', { exact: true }).count(), 0);

    const turns = page.locator('.demo-turn');
    const count = await turns.count();
    await scenario('approval');
    await send('检查项目测试');
    await page.clock.runFor(700);
    assert.equal(await page.getByRole('region', { name: '命令审批' }).isVisible(), true);
    await button('打开终端').click();
    const approvalBounds = await page.getByRole('region', { name: '命令审批' }).boundingBox();
    const conversationBounds = await page.locator('.conversation').boundingBox();
    assert.ok(approvalBounds.y + approvalBounds.height <= conversationBounds.y + conversationBounds.height + 1);
    await button('关闭终端').click();
    await send('第二轮处理文档');
    await send('这条排队消息会移除');
    await page.locator('#queue .queue-row').filter({ hasText: '这条排队消息会移除' }).getByRole('button').click();
    assert.match(await page.locator('#queue').textContent(), /后续消息 · 1/);

    await page.getByRole('button', { name: /修复断线后的提示/ }).click();
    assert.equal(await page.getByRole('region', { name: '命令审批' }).count(), 0);
    await page.getByRole('button', { name: /重整消息区布局/ }).click();
    assert.equal(await page.getByRole('region', { name: '命令审批' }).isVisible(), true);
    if (process.env.RPI_APPROVAL_SHOT) {
      await page.getByRole('region', { name: '命令审批' }).scrollIntoViewIfNeeded();
      await page.screenshot({ path: process.env.RPI_APPROVAL_SHOT });
    }
    await button('允许此会话').click();
    await page.getByLabel('执行中的消息').selectOption('steer');
    await send('本轮只关注单元测试');
    assert.equal(await turns.count(), count + 1);
    assert.match(await turns.last().textContent(), /本轮引导本轮只关注单元测试/);
    await page.clock.runFor(1100);
    assert.equal(await turns.count(), count + 2);
    assert.match(await turns.last().textContent(), /第二轮处理文档/);
    await page.clock.runFor(1800);

    await scenario('approval');
    await send('同一会话再次运行测试');
    await page.clock.runFor(700);
    assert.equal(await page.getByRole('region', { name: '命令审批' }).count(), 0);
    await page.clock.runFor(1100);

    await page.getByRole('button', { name: /修复断线后的提示/ }).click();
    await button('设置').click();
    await button('配置').click();
    await page.getByLabel('执行权限').selectOption('readonly');
    await button('关闭设置').click();
    await page.getByRole('button', { name: /重整消息区布局/ }).click();
    await send('权限变更后再次检查');
    await page.clock.runFor(700);
    assert.equal(await page.getByRole('region', { name: '命令审批' }).isVisible(), true);
    await button('允许一次').click();
    await page.clock.runFor(1100);
    await button('设置').click();
    await page.getByLabel('执行权限').selectOption('workspace');
    await button('关闭设置').click();

    await scenario('failure');
    await send('模拟工具异常');
    await page.clock.runFor(700);
    assert.match(await turns.last().textContent(), /工具执行失败/);
    await button('重试', turns.last()).click();
    await page.clock.runFor(1800);
    assert.match(await turns.last().textContent(), /本地演示已完成/);

    await button('会话操作').click();
    await page.getByLabel('会话名称').fill('新的会话名称');
    await button('保存').click();
    await button('会话操作').click();
    await button('从历史分叉').click();
    assert.equal(await page.getByRole('heading', { name: '新的会话名称（分叉）' }).isVisible(), true);
    assert.match(await turns.last().textContent(), /模拟工具异常/);
    await button('会话操作').click();
    await button('归档会话').click();
    assert.equal(await page.getByRole('button', { name: /新的会话名称（分叉）/ }).count(), 0);
    await page.reload();
    await button('已归档会话').click();
    await button('恢复').click();
    assert.equal(await page.getByRole('heading', { name: '新的会话名称（分叉）' }).isVisible(), true);
    await scenario('approval');
    await send('分叉会话需要重新审批');
    await page.clock.runFor(700);
    await button('允许一次').click();
    await page.clock.runFor(1100);
    assert.match(await turns.last().textContent(), /审批结果：允许一次/);
    await scenario('approval');
    await send('一次性权限已经用完');
    await page.clock.runFor(700);
    await button('拒绝').click();
    assert.match(await turns.last().textContent(), /已拒绝此次命令/);

    await page.getByRole('button', { name: /^新对话/ }).click();
    await page.getByLabel('执行位置').selectOption('worktree');
    await page.getByLabel('起始分支').selectOption('main');
    await send('从 main 启动隔离工作');
    assert.equal(await page.locator('#current-environment').textContent(), 'Worktree');
    assert.equal(await page.locator('#current-branch').textContent(), 'main');
    await page.getByLabel('执行中的消息').selectOption('queue');
    await send('刷新后继续这条消息');
    await page.reload();
    assert.match(await turns.last().textContent(), /演示已停止/);
    assert.match(await page.locator('#queue').textContent(), /刷新后继续这条消息/);
    await button('继续队列').click();
    await page.clock.runFor(1800);
    await button('会话操作').click();
    await button('交接到本地').click();
    assert.equal(await page.locator('#current-environment').textContent(), '本地');

    await page.setViewportSize({ width: 320, height: 550 });
    await send('窄屏运行');
    await checkViewport(page);
    await button('停止演示').click();
    assert.deepEqual(errors, []);
    assert.deepEqual(remoteRequests, []);
    console.log('Codex workflow checks passed for queue, steer, approvals, review, history and Worktree.');
  } finally {
    await page.close();
  }
}

async function checkWorkbenchFlows(browser) {
  const page = await browser.newPage({ viewport: {width:1440,height:960} });
  const errors = [];
  const requests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (!request.url().startsWith('file:')) requests.push(request.url());
  });
  try {
    await page.clock.install();
    await page.goto(url);
    const button = name => page.getByRole('button', { name, exact: true });
    const draft = page.getByRole('textbox', { name: '消息', exact: true });
    const shot = async name => {
      if (!process.env.RPI_WORKBENCH_SHOTS) return;
      require('node:fs').mkdirSync(process.env.RPI_WORKBENCH_SHOTS, { recursive: true });
      await page.screenshot({path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png')});
    };
    await shot('workspace');
    await page.getByRole('tab',{name:'文件',exact:true}).click();
    await button('composer.tsx').click();
    assert.match(await page.locator('.source-view').textContent(),/StopButton/);
    await page.getByRole('tab',{name:'变更',exact:true}).click();
    await button('打开终端').click();
    await page.getByLabel('终端命令').fill('git status');
    await page.getByLabel('终端命令').press('Enter');
    assert.match(await page.getByLabel('终端输出').textContent(),/modified: src\/components\/transcript.tsx/);
    await page.getByLabel('终端命令').fill('npm test');
    await page.getByLabel('终端命令').press('Enter');
    assert.match(await page.getByLabel('终端输出').textContent(),/2 passed/);
    await page.getByLabel('终端命令').fill('unsupported-command');
    await page.getByLabel('终端命令').press('Enter');
    assert.match(await page.getByLabel('终端输出').textContent(), /暂不支持此命令/);
    await page.getByLabel('终端命令').press('ArrowUp');
    assert.equal(await page.getByLabel('终端命令').inputValue(), 'unsupported-command');
    await shot('terminal');
    await button('关闭终端').click();
    await button('技能与插件').click();
    await shot('resources');
    await page.getByRole('tab',{name:'插件',exact:true}).click();
    await button('模拟连接').click();
    assert.equal(await button('停用').isVisible(),true);
    await page.getByRole('tab',{name:'MCP 服务',exact:true}).click();
    assert.equal(await button('停用').isVisible(),true);
    await button('停用').click();
    assert.equal(await button('模拟连接').isVisible(),true);
    await page.getByRole('tab',{name:'技能',exact:true}).click();
    await button('用于新对话').click();
    assert.match(await draft.inputValue(),/^\$code-review /);
    await button('设置').click();
    await button('配置').click();
    await page.getByLabel('执行权限').selectOption('readonly');
    await button('关闭设置').click();
    await draft.fill('检查项目');
    await draft.press('Enter');
    await page.clock.runFor(700);
    assert.equal(await page.getByRole('region',{name:'命令审批'}).isVisible(),true);
    await button('拒绝').click();
    await button('设置').click();
    await button('外观').click();
    await page.getByLabel('消息字号').selectOption('18');
    await button('环境').click();
    await page.getByLabel('测试操作').fill('cargo test');
    await button('保存环境配置').click();
    await button('连接').click();
    await page.locator('.connection-row').filter({hasText:'开发主机'}).getByRole('button').click();
    assert.match(await page.locator('.connection-row').filter({hasText:'开发主机'}).textContent(),/演示已连接/);
    await shot('settings');
    await button('关闭设置').click();
    await button('添加项目').click();
    await page.getByLabel('名称', { exact: true }).fill('rpi-work');
    await page.getByLabel('项目路径').fill('~/Projects/rpi-work');
    await button('添加项目').last().click();
    assert.equal(await page.locator('#project-name').evaluate(input => input.validity.customError), true);
    await page.getByLabel('名称',{exact:true}).fill('demo-project');
    await page.getByLabel('项目路径').fill('~/Projects/demo-project');
    await button('添加项目').last().click();
    assert.equal(await page.getByLabel('选择项目').inputValue(),'demo-project');
    await draft.fill('新项目任务');
    await draft.press('Enter');
    await page.clock.runFor(700);
    await button('允许一次').click();
    await page.clock.runFor(1100);
    await page.reload();
    assert.equal(await page.getByRole('button',{name:/新项目任务/}).count(),1);
    assert.equal(await page.locator('#font-size').inputValue(),'18');
    await button('技能与插件').click();
    await page.getByRole('tab', { name: 'MCP 服务', exact: true }).click();
    assert.equal(await button('停用').isVisible(), true);
    await page.getByRole('tab', { name: '插件', exact: true }).click();
    assert.equal(await button('停用').isVisible(), true);
    await page.getByRole('searchbox', { name: '搜索资源' }).fill('missing-resource');
    assert.equal(await page.getByText('没有匹配的资源。', { exact: true }).isVisible(), true);
    await button('计划任务').click();
    await shot('automations-empty');
    await button('新建任务').click();
    await page.getByLabel('任务名称').fill('每日代码检查');
    await page.getByLabel('任务说明').fill('审阅最近的改动并汇总测试结果。');
    await page.getByRole('dialog', { name: '新建计划任务' }).getByLabel('项目', { exact: true }).selectOption('rpi-work');
    await button('保存任务').click();
    assert.equal(await page.getByRole('heading',{name:'每日代码检查'}).isVisible(),true);
    await button('暂停').click();
    assert.equal(await page.getByText('已暂停',{exact:true}).isVisible(),true);
    await button('启用').click();
    await button('编辑').click();
    await page.getByLabel('任务名称').fill('每日变更检查');
    await button('保存任务').click();
    await button('运行演示').click();
    await page.clock.runFor(700);
    await button('允许一次').click();
    await page.clock.runFor(1100);
    assert.equal(await page.getByRole('heading',{name:'每日变更检查'}).isVisible(),true);
    await button('计划任务').click();
    assert.equal(await page.locator('.automation-run').count(), 1);
    assert.match(await page.locator('.automation-run').textContent(), /已完成/);
    await shot('automations');
    await page.reload();
    await button('计划任务').click();
    assert.equal(await page.locator('.automation-run').count(),1);
    await page.locator('.automation-run').click();
    assert.equal(await page.locator('.demo-turn').count(),1);
    for (const width of [1280,1024,700,390,320]) {
      await page.setViewportSize({width,height:700});
      for (const name of ['技能与插件','计划任务']) {
        if(width<=700) await button('切换会话侧栏').click();
        await button(name).click();
        assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
        assert.equal(await page.locator('#page-content').isVisible(),true);
      }
      if(width===390) await shot('mobile');
    }
    await button('删除').click();
    assert.equal(await page.locator('.automation-card').count(), 0);
    await page.reload();
    await button('切换会话侧栏').click();
    await button('计划任务').click();
    assert.equal(await page.locator('.automation-card').count(), 0);
    await page.evaluate(() => {
      localStorage.setItem('rpi-workbench-v1', '{broken');
      localStorage.setItem('rpi-work-layout-sessions-v1', '{broken');
    });
    await page.reload();
    assert.equal(await page.getByRole('heading', { name: '重整消息区布局' }).isVisible(), true);
    await page.addInitScript(() => {
      Object.defineProperty(window, 'localStorage', {
        configurable: true,
        get() { throw new DOMException('Disabled storage', 'SecurityError'); },
      });
    });
    await page.reload();
    await draft.fill('存储不可用时继续演示');
    await draft.press('Enter');
    await page.clock.runFor(1800);
    assert.match(await page.locator('.demo-turn').last().textContent(), /本地演示已完成/);
    assert.deepEqual(errors,[]);
    assert.deepEqual(requests,[]);
    console.log('Workspace flows passed: terminal, files, resources, permissions, settings, project persistence, scheduled task CRUD and run history, 6 viewports.');
  } finally {
    await page.close();
  }
}

async function checkDockFlows(browser) {
  const page = await browser.newPage({ viewport: { width:1680, height:1000 } });
  const errors = [], requests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (!request.url().startsWith('file:')) requests.push(request.url());
  });
  try {
    await page.goto(url);
    const button = name => page.getByRole('button', { name, exact:true });
    const shot = async name => {
      if (!process.env.RPI_WORKBENCH_SHOTS) return;
      require('node:fs').mkdirSync(process.env.RPI_WORKBENCH_SHOTS, { recursive: true });
      await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, 'dock-' + name + '.png') });
    };
    assert.equal(await page.getByRole('complementary', { name:'快捷导航' }).isVisible(), true);
    await button('切换会话侧栏').click();
    assert.equal(await page.locator('.sidebar').isVisible(),false);
    assert.equal(await page.locator('.activity-rail').isVisible(),true);
    await button('切换会话侧栏').click();
    await button('浏览器预览').click();
    const frame = page.frameLocator('iframe[title="页面预览"]');
    await frame.getByRole('heading', { name:'项目概览' }).waitFor();
    await shot('browser');
    await page.getByLabel('页面',{exact:true}).selectOption('/settings');
    await frame.getByRole('heading', { name:'项目设置' }).waitFor();
    assert.equal(await button('后退').isEnabled(),true);
    await button('后退').click();
    await frame.getByRole('heading', { name:'项目概览' }).waitFor();
    await button('前进').click();
    await frame.getByRole('heading', { name:'项目设置' }).waitFor();
    await frame.getByLabel('项目名称').fill('编辑预览');
    await button('刷新预览').click();
    assert.equal(await frame.getByLabel('项目名称').inputValue(),'RPI Work');
    await page.getByLabel('预览地址').fill('/missing');
    await page.getByLabel('预览地址').press('Enter');
    await frame.getByRole('heading', { name:'页面未收录' }).waitFor();
    await page.getByLabel('页面',{exact:true}).selectOption('/');
    await button('代码工作区').click();
    await page.getByRole('tab', { name:'composer.tsx',exact:true }).click();
    await shot('code');
    await button('编辑').click();
    const editor = page.getByRole('textbox', { name:'源码编辑器' });
    await editor.fill('export const message = "<script>fixture</script>";');
    await button('保存草稿').click();
    assert.match(await page.locator('.source-code').textContent(), /<script>fixture<\/script>/);
    assert.equal(await page.locator('.source-code script').count(),0);

    await page.reload();
    await button('代码工作区').click();
    await page.getByRole('tab', { name:'composer.tsx',exact:true }).click();
    assert.match(await page.locator('.source-code').textContent(), /<script>fixture<\/script>/);
    await button('编辑').click();
    await editor.fill('temporary change');
    await button('取消').click();
    assert.match(await page.locator('.source-code').textContent(), /fixture/);
    for (const width of [1440,1280,1080,700,390,320]) {
      await page.setViewportSize({width,height:800});
      await page.reload();
      if (!await page.locator('.inspector').isVisible()) await button('打开浏览器').click();
      await page.getByRole('tab', { name:'浏览器',exact:true }).click();
      await page.getByLabel('视口',{exact:true}).selectOption('mobile');
      await frame.getByRole('heading', { name:'项目概览' }).waitFor();
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
      if(width===390) await shot('mobile');
    }
    await page.setViewportSize({width: 1440, height: 940});
    await page.reload();
    const tab = name => page.getByRole('tab', {name, exact: true});
    await button('新建浏览器标签页').click();
    await frame.getByRole('button', {name: '设置', exact: true}).click();
    await frame.getByRole('heading', {name: '项目设置'}).waitFor();
    await tab('浏览器').click();
    await frame.getByRole('heading', {name: '项目概览'}).waitFor();
    assert.equal(await button('后退').isEnabled(), false);
    await tab('浏览器标签页 2').click();
    await frame.getByRole('heading', {name: '项目设置'}).waitFor();
    assert.equal(await button('后退').isEnabled(), true);
    await button('关闭当前标签页').click();
    await button('批注').click();
    await frame.getByRole('heading', {name: '本地开发'}).click();
    await page.getByRole('dialog', {name: '页面批注'}).waitFor();
    await page.locator('#review-comment').fill('核对开发命令');
    await button('加入下一条消息').click();
    assert.match(await page.locator('.composer').textContent(), /页面 \/ · h3 · 本地开发/);

    await frame.getByRole('button', {name: /src\/components\/composer.tsx/}).click();
    await button('编辑').click();
    await editor.fill('const unsavedDraft = 1;');
    await tab('transcript.tsx').click();
    await tab('composer.tsx').click();
    assert.equal(await editor.inputValue(), 'const unsavedDraft = 1;');
    await button('取消').click();
    await tab('变更').click();
    await button('打开源码 transcript.tsx').click();
    assert.equal(await tab('transcript.tsx').getAttribute('aria-selected'), 'true');

    await button('进入完整视图').click();
    assert.equal(await page.locator('.workspace').isVisible(), false);
    await page.keyboard.press('Control+Shift+B');
    assert.equal(await page.locator('.workspace').isVisible(), true);
    const widthOfPane = async () => (await page.locator('#inspector').boundingBox()).width;
    const widthBefore = await widthOfPane();
    const divider = page.getByRole('separator', {name: '调整右侧面板宽度'});
    await divider.focus();
    await page.keyboard.press('ArrowLeft');
    assert.equal(await widthOfPane(), widthBefore + 16);
    const rect = await divider.boundingBox();
    await page.mouse.move(rect.x + 4, rect.y + 40);
    await page.mouse.down();
    await page.mouse.move(rect.x - 65, rect.y + 40);
    await page.mouse.up();
    const draggedWidth = await widthOfPane();
    assert.ok(draggedWidth > widthBefore + 50);
    await page.reload();
    await frame.getByRole('heading', {name: '项目概览'}).waitFor();
    assert.equal(await widthOfPane(), draggedWidth);
    const fs = require('node:fs/promises');
    const directory = await fs.mkdtemp(path.join(require('node:os').tmpdir(), 'rpi-source-check-'));
    try {
      await fs.mkdir(path.join(directory, 'src'));
      await fs.writeFile(path.join(directory, 'README.md'), '# Imported project');
      const raw = 'export const localFile = "<script>raw text</script>";';
      const localSource = path.join(directory, 'src', 'local.ts');
      await fs.writeFile(localSource, raw);
      await fs.writeFile(path.join(directory, 'binary.dat'), Buffer.from([1,0,2]));
      await fs.writeFile(path.join(directory, 'large.txt'), Buffer.alloc(1024 * 1024 + 1, 65));

      await tab('文件').click();
      const chooser = page.waitForEvent('filechooser');
      await button('打开文件夹').click();
      await (await chooser).setFiles(directory);
      await page.locator('.source-code').getByText('# Imported project', {exact: true}).waitFor();
      await page.locator('.file-tree summary').filter({hasText: /^src$/}).click();
      await button('local.ts').click();
      await page.locator('.source-code').waitFor();
      assert.match(await page.locator('.source-code').textContent(), /<script>raw text<\/script>/);
      assert.equal(await page.locator('.source-code script').count(), 0);
      await button('编辑').click();
      await editor.fill('unique-local-folder-draft');
      await button('保存草稿').click();
      assert.equal(await fs.readFile(localSource, 'utf8'), raw);
      assert.equal(await page.evaluate(() => JSON.stringify(localStorage).includes('unique-local-folder-draft')), false);
      await page.getByLabel('消息', {exact: true}).fill('@local.ts');
      await page.getByRole('option', {name: /local.ts.*src\/local.ts/}).click();
      assert.ok((await page.locator('.composer').textContent()).includes('src/local.ts'));
      assert.equal((await page.locator('#transcript').textContent()).includes(raw), false);
      await button('binary.dat').click();
      await page.getByText('此文件为二进制格式，无法显示源码。', {exact: true}).waitFor();
      assert.equal(await button('编辑').count(), 0);
      await button('large.txt').click();
      await page.getByText('此文件超过 1 MiB，无法在此预览。', {exact: true}).waitFor();
      await page.getByRole('button', {name: /梳理运行时入口/}).click();
      await tab('文件').click();
      assert.equal(await button('local.ts').count(), 0);
      await page.getByRole('button', {name: /重整消息区布局/}).click();
      await tab('文件').click();
      await button('local.ts').click();
      await page.locator('.source-code').getByText('unique-local-folder-draft', {exact: true}).waitFor();
      await tab('变更').click();
      await button('打开源码 transcript.tsx').click();
      assert.equal(await tab('transcript.tsx').getAttribute('aria-selected'), 'true');
      await button('本地文件').click();
      await button('local.ts').waitFor();
      await page.reload();
      await tab('文件').click();
      assert.equal(await button('local.ts').count(), 0);
    } finally {
      await fs.rm(directory, {recursive: true, force: true});
    }
    assert.deepEqual(errors,[]);
    assert.deepEqual(requests,[]);
    console.log('Rail, browser history/refresh/routes and code editing/persistence passed, 7 viewports.');
  } finally { await page.close(); }
}

async function checkSettingsAndDefaultPanel(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  try {
    await page.clock.install();
    await page.goto(url);
    await page.frameLocator('iframe[title="页面预览"]').getByRole('heading', { name: '项目概览' }).waitFor();
    const button = name => page.getByRole('button', { name, exact: true });
    await button('设置').click();
    const bounds = await page.locator('#settings-dialog').boundingBox();
    assert.deepEqual(bounds, { x: 120, y: 78, width: 1200, height: 784 });
    const search = page.getByRole('searchbox', { name: '搜索设置' });
    await search.fill('推理强度');
    await search.press('Enter');
    const effort = page.getByLabel('默认推理强度', { exact: true });
    assert.equal(await effort.evaluate(control => document.activeElement === control), true);

    await button('外观').click();
    const theme = page.locator('#theme');
    await theme.selectOption('system');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.locator('html[data-theme="dark"]').waitFor();
    await theme.selectOption('light');
    await page.reload();
    await button('设置').click();
    await button('外观').click();
    assert.equal(await theme.inputValue(), 'light');
    assert.equal(await page.locator('html').getAttribute('data-theme'), 'light');

    await button('代码审查').click();
    await page.getByLabel('审阅会话', { exact: true }).selectOption('detached');
    await button('关闭设置').click();
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    await button('运行审阅').click();
    assert.equal(await page.getByRole('heading', { name: '审阅 · 重整消息区布局' }).isVisible(), true);
    await page.clock.runFor(1800);
    assert.equal(await page.getByText('检查空会话', { exact: true }).isVisible(), true);
    await button('设置').click();
    await button('常规').click();
    if (process.env.RPI_WORKBENCH_SHOTS) {
      await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, 'settings-desktop.png') });
    }
    await page.keyboard.press('Escape');

    await button('会话操作').click();
    await button('归档会话').click();
    await button('设置').click();
    await button('已归档的聊天').click();
    const archived = page.locator('#preference-archives .settings-row').filter({ hasText: '审阅 · 重整消息区布局' });
    await archived.getByRole('button', { name: '恢复', exact: true }).click();
    assert.equal(await archived.count(), 0);
    await button('关闭设置').click();
    assert.equal(await page.locator('.thread-button').filter({ hasText: '审阅 · 重整消息区布局' }).count(), 1);

    for (const width of [1024, 390, 320]) {
      await page.setViewportSize({ width, height: 800 });
      await page.reload();
      if (width > 900) {
        assert.equal(await page.locator('#browser-content').isVisible(), true);
        assert.equal(await page.locator('.sidebar').isVisible(), false);
      } else {
        assert.equal(await button('打开浏览器').isVisible(), true);
        assert.equal(await button('打开浏览器').textContent(), '浏览器');
        await button('打开浏览器').click();
        assert.equal(await page.locator('#browser-content').isVisible(), true);
        await button('关闭右侧面板').click();
        await button('切换会话侧栏').click();
      }
      await button('设置').click();
      const rect = await page.locator('#settings-dialog').boundingBox();
      assert.ok(Math.abs(rect.width - (width > 700 ? width * .92 : width)) < 1);
      assert.equal(rect.height, width > 700 ? 680 : 800);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
      if (width <= 700) {
        await button('浏览分类').click();
        await search.fill('代码行号');
        await search.press('Enter');
        assert.equal(await page.locator('#code-numbers').isVisible(), true);
        assert.equal(await button('浏览分类').getAttribute('aria-expanded'), 'false');
      }
      await button('关闭设置').click();
    }
    console.log('Default right browser, centered settings, search, system theme, archive restore, detached review and mobile navigation passed.');
  } finally {
    await page.close();
  }
}

async function checkReadingAndWorkflows(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.clock.install();
    await page.goto(url);
    const button = name => page.getByRole('button', { name, exact: true });
    assert.equal(await page.getByRole('heading', { name: '三种状态已补齐' }).isVisible(), true);
    await button('设置').click();
    await button('配置').click();
    await page.getByLabel('默认模型', { exact: true }).selectOption('GPT-5.3-Codex');
    await page.getByLabel('默认推理强度', { exact: true }).selectOption('高');
    await button('外观').click();
    await page.getByLabel('阅读密度', { exact: true }).selectOption('compact');
    await page.getByRole('switch', { name: '代码行号', exact: true }).uncheck();
    await button('通知').click();
    await page.getByRole('switch', { name: '任务完成提示', exact: true }).check();
    await button('关闭设置').click();

    await page.getByRole('button', { name: /^新对话/ }).click();
    assert.equal(await page.getByLabel('模型', { exact: true }).inputValue(), 'GPT-5.3-Codex');
    assert.equal(await page.getByLabel('推理强度', { exact: true }).inputValue(), '高');
    await page.getByLabel('工作模式', { exact: true }).selectOption('plan');
    const draft = page.getByRole('textbox', { name: '消息', exact: true });
    await draft.fill('组织项目测试计划');
    await draft.press('Enter');
    await page.clock.runFor(1800);
    assert.equal(await button('开始实施').isVisible(), true);
    await button('开始实施').click();
    await page.clock.runFor(1800);
    assert.equal(await page.locator('.demo-turn').count(), 2);
    assert.equal(await page.getByLabel('工作模式', { exact: true }).inputValue(), 'code');
    await button('会话操作').click();
    await button('置顶会话').click();
    assert.match(await page.locator('.thread-button[aria-current="true"]').textContent(), /置顶/);
    await page.reload();
    assert.equal(await page.locator('html').getAttribute('data-density'), 'compact');
    assert.equal(await page.locator('html').getAttribute('data-code-numbers'), 'false');
    await button('会话操作').click();
    assert.equal(await button('取消置顶').isVisible(), true);
    await page.getByRole('button', { name: '关闭会话操作', exact: true }).click();

    await page.getByRole('button', { name: /搜索会话/ }).click();
    await page.getByRole('searchbox').fill('打开终端');
    await page.getByRole('searchbox').press('Enter');
    assert.equal(await page.getByRole('region', { name: '终端演示' }).isVisible(), true);
    await button('关闭终端').click();
    await button('查看上下文').click();
    assert.match(await page.locator('#context-details').textContent(), /GPT-5.3-Codex/);
    assert.match(await page.locator('#context-details').textContent(), /消息轮次2/);
    await button('关闭上下文').click();
    assert.deepEqual(errors, []);
    console.log('Multi-turn reading, default models, display preferences, planning, implementation, pinning, commands and context passed.');
  } finally {
    await page.close();
  }
}

async function checkProjectNavigation(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  try {
    await page.goto(url);
    const button = name => page.getByRole('button', { name, exact: true });
    const pinned = page.locator('#pinned-threads');
    const recent = page.locator('#projects');
    const folder = '[data-project-group="rpi-work"]';
    const chat = '[data-session="release"]';
    const menuTrigger = button('rpi-work 项目操作');
    assert.equal(await pinned.locator(folder + ' ' + chat).count(), 1);
    await button('最近').click();
    await page.reload();
    assert.equal(await button('最近').getAttribute('aria-expanded'), 'false');
    await button('置顶').click();
    assert.equal(await pinned.isVisible(), false);
    await button('收起会话侧栏').click();
    await button('项目').click();
    assert.equal(await pinned.locator(folder + ' summary').evaluate(node => node === document.activeElement), true);

    await menuTrigger.focus();
    await page.keyboard.press('Enter');
    await page.keyboard.press('End');
    assert.equal(await page.getByRole('menuitem', { name: '取消置顶项目' }).evaluate(node => node === document.activeElement), true);
    await page.keyboard.press('Enter');
    assert.equal(await recent.locator(folder).isVisible(), true);
    assert.equal(await pinned.locator(chat).count(), 1);
    assert.equal(await recent.locator(chat).count(), 0);
    await page.reload();
    assert.equal(await recent.locator(folder).isVisible(), true);

    await menuTrigger.click();
    await page.getByRole('menuitem', { name: '置顶项目', exact: true }).click();
    assert.equal(await pinned.locator(folder + ' ' + chat).count(), 1);
    assert.equal(await page.locator('.project-scroll ' + chat).count(), 1);
    await page.reload();
    assert.equal(await pinned.locator(folder).count(), 1);
    await menuTrigger.click();
    await page.getByRole('menuitem', { name: '新建对话', exact: true }).click();
    assert.equal(await page.locator('#project-picker').inputValue(), 'rpi-work');
    await page.setViewportSize({ width: 390, height: 800 });
    await button('切换会话侧栏').click();
    await button('项目').click();
    assert.equal(await pinned.locator(folder + ' summary').evaluate(node => node === document.activeElement), true);
    console.log('Project groups, persistent pins, independent chat pins and keyboard project navigation passed.');
  } finally {
    await page.close();
  }
}

async function checkDesktopShell(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, colorScheme: 'light' });
  const button = label => page.getByRole('button', { name: label, exact: true });
  const draft = page.getByRole('textbox', { name: '消息', exact: true });
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.clock.install();
    await page.goto(url);
    const toolbar = await page.locator('.workspace-header').boundingBox();
    assert.deepEqual(toolbar, { x: 0, y: 0, width: 1440, height: 46 });
    const rail = await page.locator('.activity-rail').boundingBox();
    const nav = await page.locator('.sidebar').boundingBox();
    assert.equal(nav.width, 275);
    assert.equal(nav.x, rail.width);
    assert.equal(nav.y, 46);
    assert.equal(await page.locator('#inspector').evaluate(node => node.getBoundingClientRect().top), 46);
    await page.frameLocator('iframe').getByRole('heading', { name: '项目概览' }).waitFor();
    await shot('desktop-browser');
    await button('审阅变更').click();
    assert.equal(await page.locator('#review-content').isVisible(), true);
    await button('打开代码').click();
    await shot('desktop-code');
    await page.locator('#pinned-threads [data-session="release"]').click();
    await button('上一个会话').click();
    assert.equal(await page.locator('#current-title').innerText(), '重整消息区布局');
    await button('下一个会话').click();
    await button('上一个会话').click();
    await button('筛选会话').click();
    await page.getByLabel('显示会话').selectOption('approval');
    assert.equal(await page.getByText('没有等待审批的会话', { exact: true }).isVisible(), true);
    await button('筛选会话').click();
    await page.getByLabel('显示会话').selectOption('all');
    const original = page.locator('.assistant').first();
    await original.getByRole('button', { name: '有帮助', exact: true }).click();
    assert.equal(await original.getByRole('button', { name: '有帮助', exact: true }).getAttribute('aria-pressed'), 'true');
    await original.getByRole('button', { name: '从此处开始新对话', exact: true }).click();
    assert.equal(await page.locator('.followup-reply').count(), 0);
    await page.reload();
    assert.equal(await page.locator('.followup-reply').count(), 0);
    await page.locator('.new-chat').click();
    await page.frameLocator('iframe').getByRole('heading', { name: '项目概览' }).waitFor();
    await shot('desktop-new');
    await draft.fill('/plan');
    await draft.press('Enter');
    assert.equal(await page.locator('#collaboration-mode').inputValue(), 'plan');
    assert.equal(await page.locator('.demo-turn').count(), 0);
    await draft.fill('请检查 @compos');
    await draft.press('Tab');
    assert.equal(await draft.inputValue(), '请检查 ');
    assert.equal(await page.getByRole('button', { name: '移除 src/components/composer.tsx', exact: true }).isVisible(), true);
    await draft.fill('未发送的草稿');
    await button('添加到对话').click();
    await page.getByRole('option', { name: /设定目标/ }).click();
    await page.locator('#goal-input').fill('检查右栏开启时的消息宽度');
    await button('开始目标').click();
    assert.equal(await draft.inputValue(), '未发送的草稿');
    await button('暂停目标').click();
    assert.match(await page.locator('.demo-turn').last().innerText(), /演示已停止/);
    await button('继续目标').click();
    await page.clock.runFor(1800);
    assert.equal(await page.locator('#goal-progress').isVisible(), true);
    await page.reload();
    assert.equal(await page.locator('#goal-text').innerText(), '检查右栏开启时的消息宽度');
    await button('添加到对话').click();
    await page.getByRole('option', { name: /编辑目标/ }).click();
    await page.locator('#goal-input').fill('核对结果');
    await button('保存目标').click();
    assert.equal(await page.locator('#goal-text').innerText(), '核对结果');
    await draft.fill('/status');
    await draft.press('Enter');
    assert.equal(await page.locator('#context-dialog').isVisible(), true);
    await button('关闭上下文').click();
    await page.getByRole('button', { name: /重整消息区布局 刚刚/ }).click();
    assert.equal(await page.locator('#goal-progress').isVisible(), false);
    await button('设置').click();
    await shot('desktop-settings');
    assert.deepEqual(errors, []);
    console.log('Desktop reference dimensions, history, filters, per-reply fork, file mentions, slash commands and persistent goals passed.');
  } finally { await page.close(); }
}

async function checkDeveloperPreview(browser) {
  const http = require('node:http');
  const server = http.createServer((request, response) => {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end('<h1>Local development page</h1>');
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  try {
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, '127.0.0.1', resolve);
    });
    await page.goto(url + '?panel=code');
    assert.equal(await page.locator('#files-content').isVisible(), true);
    await page.getByRole('button', { name: '打开浏览器', exact: true }).click();
    const address = page.getByLabel('预览地址');
    const target = `http://127.0.0.1:${server.address().port}/sample`;
    await address.fill(target);
    await address.press('Enter');
    await page.frameLocator('iframe').getByRole('heading', { name: 'Local development page' }).waitFor();
    assert.equal(await page.locator('#open-address').getAttribute('href'), target);
    assert.equal(await page.getByRole('button', { name: '批注', exact: true }).isDisabled(), true);
    await page.getByRole('button', { name: '后退', exact: true }).click();
    await page.frameLocator('iframe').getByRole('heading', { name: '项目概览' }).waitFor();
    await address.fill('file:///unsupported');
    await address.press('Enter');
    assert.equal(await address.evaluate(input => input.validity.valid), false);
    await page.goto(url + '?view=settings');
    assert.equal(await page.locator('#settings-dialog').isVisible(), true);
    console.log('Live loopback page rendering, browser history and direct preview links passed.');
  } finally {
    await page.close();
    server.closeAllConnections();
    if (server.listening) await new Promise(resolve => server.close(resolve));
  }
}

async function checkMessageLayoutsAndEmptyPanel(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 940 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const button = name => page.getByRole('button', { name, exact: true });
  const shot = async name => {
    if (process.env.RPI_WORKBENCH_SHOTS) await page.screenshot({ path: path.join(process.env.RPI_WORKBENCH_SHOTS, name + '.png') });
  };
  const appearance = async () => { await button('设置').click(); await button('外观').click(); };
  try {
    await page.goto(url);
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    const transcript = await page.locator('#transcript').textContent();
    const draft = page.getByRole('textbox', { name: '消息', exact: true });
    await draft.fill('切换布局时保留的草稿');
    await page.getByText('检查了 3 个文件', { exact: true }).click();
    await page.locator('.conversation').evaluate(node => { node.scrollTop = 0; });
    const anchorTop = await page.locator('.prompt').first().evaluate(node => node.getBoundingClientRect().top);
    await shot('message-reading');
    await appearance();
    await page.getByLabel('消息布局', { exact: true }).selectOption('classic');
    await button('关闭设置').click();
    assert.ok(Math.abs(await page.locator('.prompt').first().evaluate(node => node.getBoundingClientRect().top) - anchorTop) <= 1);
    assert.equal(await page.locator('#transcript').textContent(), transcript);
    assert.equal(await draft.inputValue(), '切换布局时保留的草稿');
    assert.equal(await page.locator('.activity').first().getAttribute('open'), '');
    const geometry = await page.locator('.prompt').first().evaluate(node => ({
      x: node.getBoundingClientRect().x, width: node.getBoundingClientRect().width,
      background: getComputedStyle(node).backgroundColor, font: getComputedStyle(node).fontSize,
    }));
    const bounds = await page.locator('#transcript').boundingBox();
    assert.equal(geometry.x, bounds.x);
    assert.equal(geometry.width, bounds.width);
    assert.equal(geometry.background, 'rgba(0, 0, 0, 0)');
    assert.equal(geometry.font, '16px');
    await page.locator('.conversation').evaluate(node => { node.scrollTop = 0; });
    await shot('message-classic');
    await page.reload();
    assert.equal(await page.locator('html').getAttribute('data-message-layout'), 'classic');
    assert.equal(await page.locator('html').getAttribute('data-density'), 'comfortable');
    assert.equal(await draft.inputValue(), '切换布局时保留的草稿');
    await appearance();
    assert.equal(await page.getByLabel('消息布局', { exact: true }).inputValue(), 'classic');
    await page.getByLabel('阅读密度', { exact: true }).selectOption('compact');
    await page.getByLabel('消息字号', { exact: true }).selectOption('18');
    await page.locator('#theme').selectOption('dark');
    await button('关闭设置').click();
    await shot('message-classic-dark');
    for (const width of [1440, 1280, 1024]) {
      await page.setViewportSize({ width, height: 940 });
      await checkViewport(page);
    }
    await page.setViewportSize({ width: 1440, height: 940 });
    await appearance();
    await page.getByLabel('消息布局', { exact: true }).selectOption('reading');
    await page.locator('#theme').selectOption('light');
    await button('关闭设置').click();
    await page.reload();
    assert.equal(await page.locator('html').getAttribute('data-message-layout'), 'reading');
    assert.equal(await page.locator('html').getAttribute('data-density'), 'compact');
    assert.equal(await page.locator('.prompt').first().evaluate(node => getComputedStyle(node).fontSize), '18px');
    const prompt = await page.locator('.prompt').first().boundingBox();
    assert.ok(prompt.width < (await page.locator('#transcript').boundingBox()).width);

    const shell = page.locator('.shell');
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    const splitWidth = (await page.locator('#transcript').boundingBox()).width;
    await page.getByRole('button', { name: /^新对话/ }).click();
    assert.equal(await shell.getAttribute('data-inspector'), 'closed');
    assert.ok((await page.locator('#transcript').boundingBox()).width > splitWidth);
    await checkViewport(page);
    await shot('new-chat-empty-changes');
    await page.locator('[data-session="layout"]').click();
    assert.equal(await shell.getAttribute('data-inspector'), 'open');
    await page.getByRole('button', { name: /^新对话/ }).click();
    await button('审阅变更').click();
    assert.equal(await shell.getAttribute('data-inspector'), 'open');
    await button('关闭右侧面板').click();
    await page.locator('[data-session="layout"]').click();
    assert.equal(await shell.getAttribute('data-inspector'), 'closed');
    await button('审阅变更').click();
    await page.getByRole('button', { name: /^新对话/ }).click();
    await page.keyboard.press('Control+Shift+B');
    assert.equal(await shell.getAttribute('data-inspector'), 'open');
    await page.keyboard.press('Control+Shift+B');
    await page.locator('[data-session="layout"]').click();
    assert.equal(await shell.getAttribute('data-inspector'), 'closed');

    await button('打开浏览器').click();
    await page.getByRole('button', { name: /^新对话/ }).click();
    assert.equal(await shell.getAttribute('data-inspector'), 'open');
    assert.equal(await page.locator('#browser-content').isVisible(), true);
    await shot('new-chat-browser-preserved');
    await page.locator('[data-session="layout"]').click();
    await button('打开代码').click();
    await page.locator('.source-file-tab[aria-selected="true"]').waitFor();
    const source = await page.locator('.source-file-tab[aria-selected="true"]').textContent();
    await page.getByRole('button', { name: /^新对话/ }).click();
    assert.equal(await shell.getAttribute('data-inspector'), 'open');
    assert.equal(await page.locator('#files-content').isVisible(), true);
    assert.equal(await page.locator('.source-file-tab[aria-selected="true"]').textContent(), source);
    await shot('new-chat-source-preserved');

    await page.evaluate(() => {
      const key = 'rpi-workbench-v1';
      const saved = JSON.parse(localStorage.getItem(key));
      saved.messageLayout = 'unsupported';
      localStorage.setItem(key, JSON.stringify(saved));
    });
    await page.reload();
    assert.equal(await page.locator('html').getAttribute('data-message-layout'), 'reading');
    assert.deepEqual(errors, []);
    console.log('Reading/classic layouts, persistence, draft/details retention, desktop widths and context-sensitive new-chat panels passed.');
  } finally {
    await page.close();
  }
}

async function main() {
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const errors = [];
    const remoteRequests = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('request', request => {
      if (!request.url().startsWith('file:')) remoteRequests.push(request.url());
    });
    await page.goto(url);
    await page.getByRole('tab', { name: '变更', exact: true }).click();
    const review = page.getByRole('complementary', { name: '工作区面板' });
    const draft = page.getByRole('textbox', { name: '消息', exact: true });
    assert.equal(await page.getByRole('heading', { name: '重整消息区布局' }).count(), 1);
    await checkViewport(page);
    if (process.env.RPI_LAYOUT_SHOT) await page.screenshot({ path: process.env.RPI_LAYOUT_SHOT });
    await page.getByText('检查了 3 个文件', { exact: true }).click();
    assert.equal(await page.getByText('读取 src/components/transcript.tsx', { exact: false }).isVisible(), true);
    await page.getByRole('button', { name: /composer\.tsx/ }).first().click();
    assert.equal(await review.getByText('return busy ? <StopButton />', { exact: false }).isVisible(), true);
    await review.getByRole('button', { name: '关闭右侧面板' }).click();
    assert.equal(await review.isVisible(), false);
    await checkViewport(page);

    await draft.fill('布局草稿');
    await page.getByLabel('模型', { exact: true }).selectOption('GPT-5.3-Codex');
    await page.getByRole('button', { name: /搜索会话/ }).click();
    await page.getByRole('searchbox').fill('断线');
    await page.getByRole('searchbox').press('Enter');
    assert.equal(await draft.inputValue(), '');
    await page.getByRole('button', { name: /重整消息区布局/ }).click();
    assert.equal(await draft.inputValue(), '布局草稿');
    assert.equal(await page.getByLabel('模型', { exact: true }).inputValue(), 'GPT-5.3-Codex');

    await page.getByRole('button', { name: /^新对话/ }).click();
    assert.equal(await page.getByRole('heading', { name: /中构建什么/ }).isVisible(), true);
    await page.getByLabel('选择项目').selectOption('pi-rust');
    await page.getByRole('button', { name: '梳理项目结构' }).click();
    assert.match(await draft.inputValue(), /梳理项目结构/);
    await page.locator('#attach').setInputFiles({ name: 'note.txt', mimeType: 'text/plain', buffer: Buffer.from('fixture') });
    assert.equal(await page.getByRole('button', { name: '移除 note.txt' }).isVisible(), true);
    await draft.fill('<b>演示任务</b>');
    await draft.dispatchEvent('keydown', { key: 'Enter', isComposing: true });
    assert.equal(await page.getByRole('heading', { name: /中构建什么/ }).isVisible(), true);
    await draft.press('Enter');
    assert.equal(await page.getByRole('heading', { name: '<b>演示任务</b>' }).isVisible(), true);
    assert.equal(await page.locator('.prompt b').count(), 0);
    assert.match(await page.locator('.prompt').textContent(), /note\.txt/);
    await page.getByRole('button', { name: '停止演示' }).click();
    assert.equal(await page.getByText('演示已停止。').isVisible(), true);
    await draft.fill('完成一轮');
    await draft.press('Enter');
    await page.getByText('本地演示已完成。').waitFor();

    await page.getByRole('button', { name: '设置', exact: true }).click();
    await page.getByRole('button', { name: '外观', exact: true }).click();
    await page.getByLabel('外观', { exact: true }).selectOption('dark');
    await page.getByRole('button', { name: '关闭设置' }).click();
    assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');

    for (const viewport of [
      { width: 1280, height: 620 }, { width: 1024, height: 768 },
      { width: 700, height: 680 }, { width: 390, height: 844 }, { width: 320, height: 480 },
    ]) {
      await page.setViewportSize(viewport);
      await page.reload();
      await checkViewport(page);
      assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');
      if (viewport.width <= 900) {
        await page.getByRole('button', { name: '审阅变更', exact: true }).click();
        assert.equal(await review.isVisible(), true);
        await page.keyboard.press('Escape');
        assert.equal(await review.isVisible(), false);
      }
      if (viewport.width <= 700) {
        await page.getByRole('button', { name: '切换会话侧栏' }).click();
        await page.getByRole('button', { name: /梳理运行时入口/ }).click();
        assert.equal(await page.getByRole('heading', { name: '梳理运行时入口' }).isVisible(), true);
        await checkViewport(page);
      }
    }
    assert.deepEqual(errors, []);
    assert.deepEqual(remoteRequests, []);
    console.log('Layout and interaction checks passed at 1440, 1280, 1024, 700, 390 and 320px.');
    await checkProjectNavigation(browser);
    await checkDesktopShell(browser);
    await checkDeveloperPreview(browser);
    await checkCodexFlows(browser);
    await checkWorkbenchFlows(browser);
    await checkDockFlows(browser);
    await checkSettingsAndDefaultPanel(browser);
    await checkReadingAndWorkflows(browser);
    await checkMessageLayoutsAndEmptyPanel(browser);
    await checkHistoryViews(browser, url, checkViewport);
    await checkComposerCaret(browser, url);
    await checkReadingRestoration(browser, url);
    await checkTurnHierarchy(browser, url);
  } finally {
    await browser.close();
  }
}
main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
