// Reproduce the standalone prototype review. Uses this repository's existing Playwright.
const { chromium } = require('../../tests/support/browser.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const { pathToFileURL } = require('node:url');
const out = path.join(__dirname, 'screenshots');
const url = pathToFileURL(path.join(__dirname, 'index.html')).href;
const report = { date: '2026-10-29', engine: 'Chromium / Playwright', checks: [], geometry: [], errors: [] };
async function check(name, fn) { await fn(); report.checks.push({ name, passed: true }); }
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.on('pageerror', e => report.errors.push(e.message));
  page.on('console', m => { if (['error', 'warning'].includes(m.type())) report.errors.push(m.text()); });
  try {
    await page.goto(url);
    await check('默认完成态、空输入禁用发送、无需外部资源', async () => {
      await page.getByRole('button', { name: '查看工作区变更', exact: false }).waitFor();
      assert(await page.getByRole('button', { name: '发送消息', exact: true }).isDisabled());
      assert.equal(await page.locator('script[src],link[rel=stylesheet]').count(), 0);
    });
    await page.screenshot({ path: path.join(out, 'prototype-desktop.png') });
    await check('过程可展开；Tab 方向键即时切换；工作区范围明确', async () => {
      await page.locator('.activity summary').click();
      assert(await page.locator('.activity').evaluate(e => e.open));
      await page.locator('.activity summary').click();
      await page.getByRole('tab', { name: '对话', exact: true }).focus();
      await page.keyboard.press('ArrowRight');
      assert(await page.getByText('当前项目 Git diff · 不代表全部由此会话产生').isVisible());
      await page.getByRole('button', { name: '标记已审阅', exact: true }).click();
      assert.equal(await page.locator('#review-progress').innerText(), '1 / 3 已审阅');
      await page.locator('[data-file="1"]').click();
      assert.equal(await page.locator('#diff-path').innerText(), 'src/theme.css');
      await page.screenshot({ path: path.join(out, 'prototype-changes.png') });
      await page.getByRole('tab', { name: '对话', exact: true }).click();
    });
    await check('等待确认→授权→执行中→停止中→节点确认中断', async () => {
      await page.selectOption('#scenario', 'approval');
      assert(await page.getByRole('button', { name: '允许这一次', exact: true }).isVisible());
      await page.screenshot({ path: path.join(out, 'prototype-approval.png') });
      await page.getByRole('button', { name: '允许这一次', exact: true }).click();
      assert.match(await page.locator('#run-label').innerText(), /正在运行/);
      await page.getByRole('button', { name: '停止执行', exact: true }).click();
      assert.match(await page.locator('#run-label').innerText(), /等待节点确认/);
      await page.getByText('本轮已中断 · 可以补充指令', { exact: true }).waitFor();
    });
    await check('运行中补充消息可排队、取回编辑，不自动执行', async () => {
      await page.selectOption('#scenario', 'running');
      await page.locator('#prompt').fill('请保留系统跟随选项');
      await page.getByRole('button', { name: '排队', exact: true }).click();
      await page.locator('#queue-button').click();
      assert(await page.locator('.queue-item').isVisible());
      await page.getByRole('button', { name: '取回编辑', exact: true }).click();
      assert.equal(await page.locator('#prompt').inputValue(), '请保留系统跟随选项');
    });
    await check('断线保留草稿且禁发；重连同步；失败有修复入口', async () => {
      await page.selectOption('#scenario', 'offline');
      assert.equal(await page.locator('#prompt').inputValue(), '请保留系统跟随选项');
      assert(await page.getByRole('button', { name: '发送消息', exact: true }).isDisabled());
      await page.getByRole('button', { name: '重新连接', exact: true }).click();
      await page.getByText('本轮已完成 · 验证通过', { exact: true }).waitFor();
      await page.selectOption('#scenario', 'failed');
      await page.getByRole('button', { name: '补充修复要求', exact: true }).click();
      assert.match(await page.locator('#prompt').inputValue(), /system_preference/);
    });
    await check('Skills 选择保存；上下文抽屉 Escape 关闭并返回焦点', async () => {
      await page.getByRole('button', { name: /^Skills/ }).click();
      await page.locator('[data-skill="animate"]').check();
      await page.getByRole('button', { name: '保存选择', exact: true }).click();
      assert.equal(await page.locator('#skill-count').innerText(), '2');
      await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
      assert(await page.getByRole('dialog', { name: '任务上下文', exact: true }).isVisible());
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => document.activeElement.getAttribute('aria-label')), '查看任务上下文');
    });
    await check('搜索空态、新建任务、切换会话保留草稿、发送模拟完成', async () => {
      await page.keyboard.press('Control+k');
      await page.locator('#search-input').fill('不存在的任务');
      assert(await page.getByText('没有找到会话，试试其他关键词。').isVisible());
      await page.keyboard.press('Escape');
      await page.getByRole('button', { name: '新建任务', exact: true }).click();
      await page.locator('#new-title').fill('检查登录页面');
      await page.getByRole('button', { name: '创建并开始', exact: true }).click();
      await page.locator('#prompt').fill('草稿仍在');
      await page.locator('[data-session="architecture"]').click();
      await page.getByRole('button', { name: '检查登录页面', exact: true }).click();
      assert.equal(await page.locator('#prompt').inputValue(), '草稿仍在');
      // Selecting a scenario from another session must preserve the requested scenario.
      await page.selectOption('#scenario', 'approval');
      assert(await page.getByRole('button', { name: '允许这一次', exact: true }).isVisible());
      await page.selectOption('#scenario', 'completed');
      await page.locator('#prompt').fill('继续检查');
      await page.keyboard.press('Enter');
      await page.getByText('示例执行完成。此原型使用本地固定数据，没有调用真实模型或修改项目。', { exact: true }).waitFor();
    });
    await check('中文输入法组合 Enter 不发送；Shift Enter 换行', async () => {
      await page.locator('#prompt').fill('中文输入');
      await page.locator('#prompt').dispatchEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true });
      assert.equal(await page.locator('#prompt').inputValue(), '中文输入');
      await page.locator('#prompt').focus();
      await page.keyboard.press('End');
      await page.keyboard.press('Shift+Enter');
      assert.match(await page.locator('#prompt').inputValue(), /\n/);
    });
    await check('附件选择与移除；只读文件浏览；旁问独立', async () => {
      await page.locator('#file-input').setInputFiles({ name: '说明.txt', mimeType: 'text/plain', buffer: Buffer.from('示例') });
      assert(await page.locator('#attachment-list').getByText('说明.txt').isVisible());
      await page.getByRole('button', { name: '移除附件 说明.txt', exact: true }).click();
      await page.getByRole('button', { name: '打开项目文件', exact: true }).click();
      await page.locator('[data-preview="0"]').click();
      assert.match(await page.locator('#file-content').innerText(), /system_theme/);
      await page.keyboard.press('Escape');
      await page.getByRole('button', { name: '更多会话操作', exact: true }).click();
      await page.getByRole('button', { name: 'BTW 旁问', exact: true }).click();
      await page.locator('#aside-input').fill('为什么跟随系统');
      await page.getByRole('button', { name: '发送旁问', exact: true }).click();
      assert.match(await page.locator('#aside-answer').innerText(), /固定演示文案/);
      await page.keyboard.press('Escape');
    });
    await page.reload();
    await check('1440 / 1024 / 768 / 390 / 320 宽度没有页面横向溢出', async () => {
      for (const [width, height] of [[1440,1000],[1024,768],[768,1024],[390,844],[320,740]]) {
        await page.setViewportSize({ width, height });
        const g = await page.evaluate(() => ({ width: innerWidth, documentWidth: document.documentElement.scrollWidth, composer: document.querySelector('.composer').getBoundingClientRect().toJSON(), inputSize: getComputedStyle(document.querySelector('#prompt')).fontSize }));
        assert(g.documentWidth <= width);
        assert(g.composer.right <= width && g.composer.left >= 0);
        assert(g.composer.bottom <= height);
        if (width <= 700) assert.equal(g.inputSize, '16px');
        report.geometry.push(g);
        for (const scenario of ['approval','running','offline','failed','completed']) {
          await page.selectOption('#scenario', scenario);
          await page.locator('#prompt').fill(scenario==='running'?'测试补充消息':'');
          const fits = await page.locator('.composer-tools').evaluate(e => [...e.children].filter(n=>n.getBoundingClientRect().width).every(n=>n.getBoundingClientRect().right<=innerWidth));
          assert(fits, `${width}px ${scenario}: composer controls overflow`);
          assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth),width);
        }
        await page.screenshot({ path: path.join(out, `prototype-${width}.png`) });
      }
    });
    await page.setViewportSize({ width:390, height:844 });
    await page.screenshot({ path: path.join(out, 'prototype-mobile.png') });
    await check('移动导航可关闭；变更列表/详情返回；弹窗适配', async () => {
      await page.getByRole('button', { name: '打开项目导航', exact: true }).click();
      await page.locator('[data-session="mobile"]').click();
      assert.equal(await page.locator('#sidebar').evaluate(e => e.inert), true);
      await page.getByRole('tab', { name: /^变更/ }).click();
      await page.locator('[data-file="2"]').click();
      assert(await page.locator('#diff-code').isVisible());
      await page.screenshot({ path: path.join(out, 'prototype-mobile-diff.png') });
      await page.getByRole('button', { name: '返回变更文件列表', exact: true }).click();
      assert(await page.locator('#change-files').isVisible());
      await page.getByRole('tab', { name: '对话', exact: true }).click();
      await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
      assert((await page.locator('#dialog').boundingBox()).width <= 390);
      await page.screenshot({ path: path.join(out, 'prototype-mobile-context.png') });
      await page.keyboard.press('Escape');
    });
    await check('深色主题；减少动态效果；交互控件具备可访问名称', async () => {
      await page.getByRole('button', { name: '切换深浅主题', exact: true }).click();
      assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');
      await page.screenshot({ path: path.join(out, 'prototype-mobile-dark.png') });
      await page.emulateMedia({ reducedMotion:'reduce' });
      await page.getByRole('button', { name: '查看任务上下文', exact: true }).click();
      assert.equal(await page.locator('#dialog').evaluate(e=>getComputedStyle(e).animationName), 'none');
      await page.keyboard.press('Escape');
      const unnamed = await page.locator('button').evaluateAll(nodes => nodes.filter(n => !n.textContent.trim() && !n.getAttribute('aria-label')).length);
      assert.equal(unnamed, 0);
    });
    assert.deepEqual(report.errors, []);
    fs.writeFileSync(path.join(__dirname, 'verification.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify({ passed: report.checks.length, errors: report.errors, widths: report.geometry.map(g=>g.width) }, null, 2));
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
