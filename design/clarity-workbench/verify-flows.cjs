// End-to-end acceptance of the local HTML prototype, with all network blocked.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('../../tests/support/browser.cjs');

(async () => {
  const output = path.resolve('artifacts/clarity-flows');
  fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  page.setDefaultTimeout(7000);
  const results = [], errors = [], network = [];
  page.on('pageerror', error => errors.push(error.message));
  await context.route(/^https?:/, route => { network.push(route.request().url()); return route.abort(); });
  const button = (name, scope = page) => scope.getByRole('button', { name, exact: true });
  const pass = name => { results.push(name); console.log('PASS', name); };
  const work = page.locator('#work-dialog');
  const resource = key => page.locator(`.flow-resource[data-resource-key="${key}"]`);
  const navigate = async kind => {
    if (await page.evaluate(() => innerWidth < 768)) await button('打开导航').click();
    await page.locator('.sidebar [data-resource-entry="all"]').click();
    await page.locator(`[data-kind="${kind}"]`).click();
  };
  const select = async key => {
    const row = resource(key);
    const choose = row.locator('[data-flow=select-tool]');
    if (await choose.getAttribute('aria-pressed') !== 'true') await choose.click();
  };
  const startPreparation = async (scenario = 'success', destination = 'new', scope = 'project') => {
    await page.locator('.selection-panel [data-flow=checkout]').click();
    if (await work.locator('#work-destination').count()) await work.locator('#work-destination').selectOption(destination);
    if (scenario !== 'success') {
      await work.locator('.flow-scenarios summary').click();
      await work.locator('#work-scenario').selectOption(scenario);
    }
    await button('预览准备过程', work).click();
    if (await work.locator('[name=install-scope]').count()) {
      await work.locator(`[name=install-scope][value=${scope}]`).check();
      await button('确认并演示安装', work).click();
    }
  };
  const finishPreparation = async () => {
    for (let index = 0; index < 12; index++) {
      const advance = button('推进演示', work);
      if (await advance.count()) { await advance.click(); continue; }
      const retry = work.locator('[data-flow=recover-preparation]');
      if (await retry.count()) { await retry.click(); continue; }
      const resume = button('继续工作准备', work);
      if (await resume.count()) { await resume.click(); continue; }
      return;
    }
    throw new Error('Preparation failed to reach its final action');
  };
  const noOverflow = async () => assert(await page.evaluate(() => {
    const active = document.querySelector('.page:not([hidden])');
    return document.documentElement.scrollWidth <= innerWidth
      && active.scrollWidth <= active.clientWidth + 1
      && [...document.querySelectorAll('dialog[open]')].every(dialog => dialog.scrollWidth <= dialog.clientWidth + 1);
  }), 'page and dialog must fit the viewport');
  try {
    await page.goto(pathToFileURL(path.join(__dirname, 'index.html')).href + '#resources');
    const scope = page.locator('#installed-scope');
    const scopeValue = (node, project) => JSON.stringify([node, project]);
    const count = async kind => Number(await page.locator(`[data-count="${kind}"]`).innerText());
    assert.equal(await page.locator('.sidebar [data-resource-entry]').count(), 1);
    assert.equal(await page.locator('.advanced-navigation').count(), 0);
    assert.match(await page.locator('#resource-count-dimension').innerText(), /公共库.*10/);
    assert.equal(await count('skills'), 4);
    await page.locator('#resource-search').fill('代码审查');
    assert.equal(await count('skills'), 4);
    assert.match(await page.locator('#resource-result-count').innerText(), /显示 1 \/ 共 10/);
    await page.locator('#resource-search').fill('');
    await page.locator('[data-source="已安装"]').click();
    assert.equal(await scope.inputValue(), scopeValue('local', '*'));
    assert.equal(await page.locator('.flow-resource').count(), 2);
    assert.equal(await count('skills'), 0);
    await scope.selectOption(scopeValue('local', 'rpi-work'));
    assert.equal(await page.locator('.flow-resource').count(), 5);
    assert.equal(await count('packages'), 2);
    assert.match(await resource('packages/base').innerText(), /项目安装 · 全局也可用/);
    await resource('packages/base').locator('[data-flow=detail]').click();
    await button('移除项目安装', page.locator('#detail-dialog')).click();
    await button('确认移除', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('.flow-resource').count(), 5);
    assert.match(await resource('packages/base').innerText(), /全局可用/);
    await resource('packages/base').locator('[data-flow=detail]').click();
    assert.equal(await page.locator('#detail-dialog [data-flow=uninstall]').count(), 0);
    await page.keyboard.press('Escape');
    await scope.selectOption(scopeValue('local', '*'));
    await resource('packages/base').locator('[data-flow=detail]').click();
    await button('移除全局安装', page.locator('#detail-dialog')).click();
    await button('确认移除', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('.flow-resource').count(), 1);
    await scope.selectOption(scopeValue('local', 'rpi-work'));
    assert.equal(await page.locator('.flow-resource').count(), 4);
    await scope.selectOption(scopeValue('studio', '*'));
    assert.equal(await page.locator('.flow-resource').count(), 0);
    await page.reload();
    assert.equal(await scope.inputValue(), scopeValue('studio', '*'));
    assert.equal(await page.locator('.flow-resource').count(), 0);
    await page.locator('[data-source="公共库"]').click();
    assert.equal(await page.locator('.flow-resource').count(), 10);
    pass('single center exposes catalog counts and explicit node/global/project counts with inherited-version deduplication and exact-scope removal');

    const sessionsBefore = await page.locator('#sidebar-sessions [data-session-id^=preview-]').count();
    await resource('skills/code-review').locator('[data-flow=install]').click();
    assert(await button('确认并演示安装', work).isDisabled());
    await work.locator('#install-node').selectOption('local');
    await work.locator('[name=install-scope][value=global]').check();
    await page.screenshot({ path: path.join(output, 'install-scope-desktop.png'), animations: 'disabled' });
    await button('取消', work).click();
    await page.locator('[data-source="已安装"]').click();
    await scope.selectOption(scopeValue('local', '*'));
    assert.equal(await page.locator('.flow-resource').count(), 2);
    await page.locator('[data-source="公共库"]').click();
    await resource('skills/code-review').locator('[data-flow=install]').click();
    await work.locator('[name=install-scope][value=global]').check();
    await button('确认并演示安装', work).click();
    await finishPreparation();
    await button('完成安装', work).click();
    assert.equal(await scope.inputValue(), scopeValue('local', '*'));
    assert.equal(await page.locator('.flow-resource').count(), 3);
    assert.equal(await count('skills'), 1);
    assert.equal(await page.locator('#selected-list .selected-tool').count(), 0);
    assert.equal(await page.locator('#sidebar-sessions [data-session-id^=preview-]').count(), sessionsBefore);
    await scope.selectOption(scopeValue('local', 'pi-rust'));
    assert.equal(await page.locator('.flow-resource').count(), 3);
    assert.match(await resource('skills/code-review').innerText(), /全局可用/);
    await scope.selectOption(scopeValue('studio', 'pi-rust'));
    assert.equal(await page.locator('.flow-resource').count(), 0);
    await page.locator('[data-source="公共库"]').click();
    await resource('skills/dioxus-knowledge-patch').locator('[data-flow=install]').click();
    await work.locator('#install-node').selectOption('local');
    await work.locator('[name=install-scope][value=project]').check();
    await work.locator('#install-project').selectOption('pi-rust');
    await button('确认并演示安装', work).click();
    await finishPreparation();
    await button('完成安装', work).click();
    assert.equal(await scope.inputValue(), scopeValue('local', 'pi-rust'));
    assert.equal(await page.locator('.flow-resource').count(), 4);
    await scope.selectOption(scopeValue('local', '*'));
    assert.equal(await page.locator('.flow-resource').count(), 3);
    await scope.selectOption(scopeValue('local', 'rpi-work'));
    assert.equal(await page.locator('.flow-resource').count(), 6);
    assert.equal(await resource('skills/dioxus-knowledge-patch').count(), 0);
    await page.locator('#resources-page').evaluate(element => { element.scrollTop = 0; });
    await page.screenshot({ path: path.join(output, 'installed-project-desktop.png'), animations: 'disabled' });
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 844 });
      await noOverflow();
      await page.locator('#resources-page').evaluate(element => { element.scrollTop = 0; });
      await page.screenshot({ path: path.join(output, `installed-project-${width}.png`), animations: 'disabled' });
      await resource('skills/code-review').locator('[data-flow=install]').click();
      await work.locator('[name=install-scope][value=project]').check();
      await noOverflow();
      await page.screenshot({ path: path.join(output, `install-scope-${width}.png`), animations: 'disabled' });
      await button('取消', work).click();
    }
    await page.setViewportSize({ width: 1440, height: 1000 });
    pass('install popup requires scope choice, cancellation retains counts, global installation inherits within its node, project installation stays within its project');

    await page.goto(pathToFileURL(path.join(__dirname, 'index.html')).href + '#workspace');
    await page.reload();
    await page.locator('#prompt').fill('原会话的草稿需要保留');
    await navigate('all');
    assert.equal(await page.locator('.flow-resource').count(), 10);
    assert(await page.locator('.selection-panel [data-flow=checkout]').isDisabled());
    await select('packages/developer');
    await navigate('skills');
    assert.equal(await page.locator('.flow-resource').count(), 4);
    await select('skills/code-review');
    await navigate('extensions');
    await select('extensions/codegraph');
    assert.equal(await page.locator('#selected-list .selected-tool').count(), 3);
    assert.equal(await page.locator('.sidebar .nav-link[aria-current=page]').count(), 1);
    await navigate('packages');
    await resource('packages/base').locator('[data-flow=select-tool]').click();
    await button('确认替换', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('#selected-list .selected-tool').count(), 3);
    assert.match(await page.locator('#selected-list').innerText(), /工作基础/);
    assert.doesNotMatch(await page.locator('#selected-list').innerText(), /开发基础/);
    pass('overview and internal type filters share selections, with explicit single-Package replacement');

    await page.locator('.selection-panel [data-flow=checkout]').click();
    await work.locator('#work-model').selectOption('GPT-5');
    assert(await button('预览准备过程', work).isDisabled());
    await button('使用示例模型配置', work).click();
    await work.locator('#work-destination').selectOption('new');
    await work.locator('#work-intent').fill('检查项目中的边界条件 <img src=x onerror=alert(1)>');
    await button('返回选择', work).click();
    assert.equal(await page.locator('#selected-list .selected-tool').count(), 3);
    await page.locator('#library-return [data-flow=return-origin]').click();
    assert.equal(await page.locator('#prompt').inputValue(), '原会话的草稿需要保留');
    pass('missing model setup is actionable and cancelled preparation retains selection and original draft');

    await navigate('all');
    await page.locator('.selection-panel [data-flow=checkout]').click();
    await work.locator('#work-intent').fill('保留工作设置中的输入');
    await button('预览准备过程', work).click();
    assert(await work.locator('[name=install-scope]').count() > 0);
    await page.keyboard.press('Escape');
    assert.equal(await work.locator('#work-intent').inputValue(), '保留工作设置中的输入');
    await button('返回选择', work).click();
    await startPreparation('download-failed', 'new', 'global');
    await button('推进演示', work).click();
    assert.match(await work.getByRole('status').innerText(), /获取失败/);
    await button('重试此步骤', work).click();
    await finishPreparation();
    await button('进入新会话', work).click();
    assert.equal(await page.locator('#preview-transcript .user-message').count(), 0);
    assert.match(await page.locator('#work-tools-bar').getAttribute('title'), /代码审查/);
    const firstNewSession = await page.locator('#sidebar-sessions .sidebar-recent [data-session-id^=preview-]').first().getAttribute('data-session-id');
    await page.locator('#prompt').fill('审查代码 <img src=x onerror=alert(1)>');
    await page.keyboard.press('Enter');
    assert.equal(await page.locator('#preview-transcript .user-message').count(), 1);
    assert.equal(await page.locator('#preview-transcript img').count(), 0);
    await page.locator('#prompt').fill('运行中的补充草稿');
    await button('演示等待确认').click();
    await button('查看并回答').click();
    await button('允许本次', page.locator('#flow-dialog')).click();
    await button('演示断线').click();
    await button('重连并核对').click();
    await button('查看变更', page.locator('#run-preview')).click();
    assert(await page.locator('#changes-panel').isVisible());
    await page.locator('#changes-panel [data-tab=chat]').click();
    assert.equal(await page.locator('#prompt').inputValue(), '运行中的补充草稿');
    await button('继续对话', page.locator('#run-preview')).click();
    pass('preparation retries reach a new conversation, preserving safe text, confirmation, reconnect and review flows');

    await button('查看会话工具', page.locator('#work-tools-bar')).click();
    await button('调整工具', page.locator('#flow-dialog')).click();
    await navigate('extensions');
    await select('extensions/lens');
    await startPreparation('install-unknown', 'current');
    for (let index = 0; index < 3; index++) await button('推进演示', work).click();
    assert.match(await work.getByRole('status').innerText(), /结果待核对/);
    await button('核对演示状态', work).click();
    await finishPreparation();
    await button('返回当前会话', work).click();
    assert.equal(await page.locator('#sidebar-sessions .sidebar-recent [data-session-id^=preview-]').first().getAttribute('data-session-id'), firstNewSession);
    assert.equal(await page.locator('#prompt').inputValue(), '运行中的补充草稿');
    assert.match(await page.locator('#work-tools-bar').getAttribute('title'), /开发诊断/);
    pass('unknown installation requires readback and applies to the original session without a duplicate or draft loss');

    await button('查看会话工具', page.locator('#work-tools-bar')).click();
    await button('调整工具', page.locator('#flow-dialog')).click();
    await startPreparation('apply-failed', 'current');
    assert.equal(await work.locator('[name=install-scope]').count(), 0);
    await button('推进演示', work).click();
    assert.match(await work.getByRole('status').innerText(), /应用失败/);
    await finishPreparation();
    await button('返回当前会话', work).click();
    assert.equal(await page.locator('#prompt').inputValue(), '运行中的补充草稿');
    pass('available tools skip installation scope and application retries preserve the current session and draft');

    await button('查看会话工具', page.locator('#work-tools-bar')).click();
    await button('调整工具', page.locator('#flow-dialog')).click();
    await page.locator('.selection-panel [data-flow=project-defaults]').click();
    await button('预览准备过程', work).click();
    await finishPreparation();
    await button('保存默认组合并返回', work).click();
    await page.keyboard.press('Control+j');
    assert.match(await page.locator('#work-tools-bar').getAttribute('title'), /项目默认工具.*代码审查/);
    await page.locator('#prompt').fill('使用项目默认组合开始新任务');
    await page.keyboard.press('Enter');
    assert.match(await page.locator('#work-tools-bar').getAttribute('title'), /开发诊断/);
    pass('project defaults carry into later home-created conversations');

    await button('会话操作', page.locator('#work-tools-bar')).click();
    await page.locator('#rename-session').fill('工具流程验收会话');
    await button('保存名称', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('#session-title').innerText(), '工具流程验收会话');
    await button('会话操作', page.locator('#work-tools-bar')).click();
    await button('归档会话', page.locator('#flow-dialog')).click();
    await button('已归档', page.locator('.sidebar')).click();
    await page.locator('#flow-dialog [data-flow=restore-session]').filter({ hasText: '工具流程验收会话' }).click();
    assert.equal(await page.locator('#session-title').innerText(), '工具流程验收会话');
    pass('rename, archive and restore are connected to the same project and session');

    await navigate('all');
    await button('导入设计').click();
    await page.locator('#design-json').fill('{"id":"broken"}');
    await button('校验并导入').click();
    assert.match(await page.locator('#design-error').innerText(), /有效/);
    await button('填入示例').click();
    await button('校验并导入').click();
    assert.match(await page.locator('#resource-grid').innerText(), /我的审查技能/);
    const downloadPromise = page.waitForEvent('download');
    await page.locator('#resource-grid [data-flow=export-design]').filter({ visible: true }).last().click();
    assert.equal((await downloadPromise).suggestedFilename(), 'my-review.json');
    await page.locator('#resource-grid [data-flow=edit-design]').last().click();
    await button('静态校验').click();
    await button('预览测试').click();
    await button('演示测试通过').click();
    assert(!await button('发布本地演示版本').isDisabled());
    await page.locator('#author-source').fill('# 我的审查\n\n修改后需要重新检查。');
    assert(await button('发布本地演示版本').isDisabled());
    await button('静态校验').click();
    await button('预览测试').click();
    await button('演示测试失败').click();
    assert(await button('发布本地演示版本').isDisabled());
    await button('预览测试').click();
    await button('演示测试通过').click();
    await button('发布本地演示版本').click();
    await button('查看发布版本').click();
    assert.match(await page.locator('#detail-dialog').innerText(), /本地演示发布/);
    await button('加入工具组合', page.locator('#detail-dialog')).click();
    await page.keyboard.press('Escape');
    assert.match(await page.locator('#selected-list').innerText(), /我的审查技能/);
    await button('开发工具').click();
    await page.locator('#author-version').fill('1.1.0');
    await button('静态校验').click();
    await button('预览测试').click();
    await button('演示测试通过').click();
    await button('发布本地演示版本').click();
    await button('查看发布版本').click();
    await button('加入工具组合', page.locator('#detail-dialog')).click();
    await button('确认替换版本', page.locator('#flow-dialog')).click();
    assert.match(await page.locator('#selected-list').innerText(), /v1.1.0/);
    assert.equal(await page.locator('#selected-list .selected-tool').filter({ hasText: '我的审查技能' }).count(), 1);
    pass('validated import/export and revision-bound development/test/publication lead back to tool selection');

    await navigate('all');
    await page.locator('[data-source="公共库"]').click();
    await page.locator('#resource-search').fill('不存在的工具');
    assert(await page.locator('.resource-empty').isVisible());
    await button('清除筛选').click();
    assert.equal(await page.locator('.flow-resource').count(), 10);
    await page.locator('#resource-search').fill('代码审查');
    assert.equal(await page.locator('.flow-resource').count(), 1);
    await page.locator('#resource-search').fill('');
    for (const [width, height] of [[1920, 1080], [1440, 1000], [1024, 768], [768, 768], [390, 844], [320, 568]]) {
      await page.setViewportSize({ width, height });
      await noOverflow();
      await page.locator('#resources-page').evaluate(element => { element.scrollTop = 0; });
      await page.screenshot({ path: path.join(output, `resource-center-${width}.png`), animations: 'disabled' });
      if (width <= 980) {
        await page.locator('.selection-dock [data-flow=review-selection]').click();
        assert(await page.locator('#selection-dialog .selected-tool').count() >= 3);
        await noOverflow();
        await page.keyboard.press('Escape');
      }
    }
    await page.locator('.selection-dock [data-flow=checkout]').click();
    await noOverflow();
    await page.screenshot({ path: path.join(output, 'work-prepare-320.png'), animations: 'disabled' });
    await page.keyboard.press('Escape');
    pass('search, category routing, persistent selection and preparation fit desktop through 320px screens');

    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.locator('.prototype-flow-entry').click();
    await page.locator('#flow-dialog [data-flow=onboarding]').click();
    for (const name of ['演示缺少 rpi', '演示环境准备完成', '使用示例模型配置', '选择项目并继续']) {
      await button(name, page.locator('#flow-dialog')).click();
    }
    await page.locator('#flow-dialog .flow-scenarios summary').click();
    await page.locator('#flow-project-name').fill('prototype-project');
    await button('添加演示项目', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('#welcome-project-name').innerText(), 'prototype-project');
    await page.getByRole('button', { name: '在本机的rpi-work中新建会话', exact: true }).click();
    await page.locator('#new-session-title').fill('从项目直接开始');
    await button('进入新会话', page.locator('#new-session-dialog')).click();
    assert.equal(await page.locator('#session-title').innerText(), '从项目直接开始');
    await button('会话操作', page.locator('#work-tools-bar')).click();
    await button('选择执行后端', page.locator('#flow-dialog')).click();
    await page.locator('#backend-choice').selectOption('Codex');
    await button('确认新建会话', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('#session-title').innerText(), 'Codex 工作会话');
    pass('first-run preparation, project addition, direct session creation and backend selection have usable endpoints');

    await page.locator('.sidebar [data-view=settings]').click();
    await page.locator('[data-settings="供应商"]').click();
    await button('添加供应商', page.locator('#providers-panel')).click();
    await page.getByLabel('供应商 ID', { exact: true }).fill('prototype-provider');
    await page.getByLabel('显示名称', { exact: true }).fill('示例供应商');
    await button('仅保存供应商定义', page.locator('#provider-definition-dialog')).click();
    const provider = page.locator('[data-saved-provider="prototype-provider"]');
    await button('配置密钥', provider).click();
    await page.getByLabel('新 API Key', { exact: true }).fill('demonstration-only-value');
    await button('仅保存 API Key', page.locator('#provider-key-dialog')).click();
    assert.equal(await page.getByLabel('新 API Key', { exact: true }).inputValue(), '');
    assert.match(await provider.innerText(), /已配置/);
    await page.locator('.prototype-flow-entry').click();
    await page.locator('#flow-dialog [data-flow=connect-device]').click();
    await page.locator('#connect-node').selectOption('studio');
    await button('连接演示节点', page.locator('#flow-dialog')).click();
    assert.equal(await page.locator('#welcome-project-node').innerText(), '开发机');
    pass('provider setup and device switching update local demonstration state without retaining credential input');
    assert.deepEqual(errors, []);
    assert.deepEqual(network, []);
    pass('all flows run locally without network calls or browser errors');
    fs.writeFileSync(path.join(output, 'validation.json'), JSON.stringify({ results, errors, network }, null, 2));
  } catch (error) {
    await page.screenshot({ path: path.join(output, 'failure.png'), animations: 'disabled' });
    fs.writeFileSync(path.join(output, 'validation.json'), JSON.stringify({ results, errors, network, fatal: error.stack }, null, 2));
    throw error;
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
