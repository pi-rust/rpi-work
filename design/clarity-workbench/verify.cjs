// Standalone design prototype checks, not product or native-client acceptance.
const { chromium } = require('../../tests/support/browser.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const output = path.join(__dirname,'screenshots','settings-collaboration');
fs.mkdirSync(output,{recursive:true});
(async () => {
  const browser = await chromium.launch({headless:true});
  const context = await browser.newContext({viewport:{width:1440,height:1000},reducedMotion:'reduce'});
  const page = await context.newPage(); page.setDefaultTimeout(6000);
  const errors = [], network = [], checks = [], workspaceGeometry = [];
  page.on('pageerror', error => errors.push(error.message));
  await context.route(/^https?:/, route => {network.push(route.request().url());route.abort();});
  const check = async (name,body) => { await body();checks.push(name);console.log('PASS',name); };
  const capture = async name => {
    await page.evaluate(() => document.activeElement?.blur());
    await page.screenshot({path:path.join(output,name+'.png')});
  };
  const view = name => name === 'workspace' ? page.locator('.sidebar .brand').click() : page.locator(`.main-nav button[data-view="${name}"]`).click();
  const noOverflow = async () => {
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),'No document horizontal overflow');
    assert(await page.locator('.page:not([hidden])').evaluate(element => element.scrollWidth <= element.clientWidth + 1),'No active-page horizontal overflow');
  };
  const navigate = async name => {
    if (!await page.locator('.sidebar').isVisible()) await page.getByRole('button',{name:'打开导航',exact:true}).click();
    if(name === 'workspace') await page.locator('.sidebar .brand').click();
    else await page.locator(`.sidebar .nav-link[data-view="${name}"]`).click();
  };
  const setSessionDisplay = async (mode,screenshot) => {
    await navigate('settings');
    await page.locator('[data-settings-panel="session-display"]').click();
    await page.getByLabel('会话展示方式',{exact:true}).selectOption(mode);
    assert.equal(await page.getByLabel('会话展示方式',{exact:true}).inputValue(),mode);
    await noOverflow();if(screenshot)await capture(screenshot);
    await page.locator('[data-settings-panel="appearance"]').click();
    await navigate('workspace');
  };
  try {
    await page.goto(pathToFileURL(path.join(__dirname,'index.html')).href);
    await check('独立静态原型，默认会话与禁用发送',async () => { await page.locator('#session-title').getByText('重新设计工作台界面',{exact:true}).waitFor();assert(await page.getByRole('button',{name:'发送消息',exact:true}).isDisabled());await noOverflow(); });
    await capture('01-workspace-desktop');
    await check('项目文件入口同时展示目录与改动，摘要在侧栏打开 Diff 并保留输入草稿',async () => {await page.locator('#prompt').fill('保留这份草稿');await page.locator('.project-files-trigger').click();await assert(await page.locator('#project-files-panel').isVisible());const panelBox=await page.locator('#project-files-panel').boundingBox();const viewportWidth=await page.evaluate(()=>innerWidth);assert(panelBox.x+panelBox.width>=viewportWidth-1 && panelBox.x>viewportWidth/2,'项目文件面板位于右侧');await assert(await page.getByText('项目目录',{exact:true}).isVisible());await assert(await page.getByText('工作区改动',{exact:true}).isVisible());await capture('37-project-files-desktop');await page.locator('#project-files-panel .project-changes .project-file-row.changed').first().click();await assert(await page.locator('#project-file-diff').isVisible());assert(await page.locator('#changes-panel').isHidden());assert((await page.locator('.project-files-panel').boundingBox()).width>600,'文件 Diff 使用大侧栏');await page.locator('[data-file-tab="theme_contract.rs"]').click();assert.match(await page.locator('#project-file-diff-path').innerText(),/theme_contract\.rs/);await page.locator('[data-file-tab="appearance.rs"]').click();await capture('38-project-file-diff-sidebar-desktop');await page.getByRole('button',{name:'返回文件列表',exact:true}).click();assert(await page.getByText('工作区改动',{exact:true}).isVisible());await page.locator('#project-files-panel [data-action="project-files"]').click();await page.locator('.change-summary').click();await assert(await page.locator('#project-file-diff').isVisible());assert(await page.locator('#changes-panel').isHidden());await page.locator('#project-files-panel [data-action="project-file-diff-back"]').click();await page.locator('#project-files-panel [data-action="project-files"]').click();assert.equal(await page.locator('#prompt').inputValue(),'保留这份草稿');});
    await check('模型与思考常驻，直接切换并保留草稿；工具过程可展开',async () => {
      await page.locator('.tool-process summary').click();assert(await page.locator('.tool-detail').isVisible());await page.locator('.tool-process summary').click();
      assert(await page.locator('.composer .model-button').isVisible());assert(await page.locator('.composer .thinking-button').isVisible());
      await page.getByRole('button',{name:'切换模型',exact:true}).click();await page.locator('[data-model="GPT-5"]').click();
      assert.equal(await page.locator('.model-name').innerText(),'GPT-5');
      await page.getByRole('button',{name:'切换模型',exact:true}).click();await page.locator('[data-model="DeepSeek V4 Pro"]').click();
      await page.getByRole('button',{name:'切换思考等级',exact:true}).click();await page.locator('[data-thinking="中"]').click();assert.match(await page.locator('.thinking-name').innerText(),/中/);
      await page.getByRole('button',{name:'切换思考等级',exact:true}).click();await page.locator('[data-thinking="高"]').click();
      assert.equal(await page.locator('#prompt').inputValue(),'保留这份草稿');
    });
    await check('详细命令与简洁显示可切换；错误始终可见，命令和参数原文保留且草稿不变',async () => {
      const draft=await page.locator('#prompt').inputValue();
      assert(await page.locator('.tool-attention').isVisible());
      assert.match(await page.locator('.command-call[data-result="failed"]>summary').innerText(),/失败 · 101/);
      assert.equal(await page.locator('.command-body:visible').count(),0);
      await page.locator('[data-action="tool-details"]').click();
      assert.equal(await page.locator('#tool-records').getAttribute('data-rendering'),'detailed');
      assert.equal(await page.locator('#tool-call-list>details[open]').count(),2);
      assert(!await page.locator('.tool-process').evaluate(el=>el.open));
      assert.equal(await page.locator('.command-body:visible').count(),2);
      assert.equal(await page.locator('.command-body h3,.command-stream-note').count(),0);
      assert.equal(await page.locator('.command-output').count(),1);
      assert.equal(await page.locator('.command-origin:visible,.command-arguments>pre:visible').count(),0);
      for(const card of await page.locator('.command-call').all()) {
        await card.locator('.command-arguments>summary').click();
        const parameters=JSON.parse(await card.locator('.command-arguments>pre').innerText());
        assert.equal(parameters.command,await card.locator('.command-text').innerText());assert.equal(parameters.timeout,120);
        await card.locator('.command-arguments>summary').click();
      }
      await page.locator('#tool-records').scrollIntoViewIfNeeded();await capture('22-detailed-commands-desktop');
      await page.locator('.command-call[data-result="failed"]').scrollIntoViewIfNeeded();await capture('23-failed-command-desktop');
      for(const width of [1440,1280,1024,768,390,320]) {
        await page.setViewportSize({width,height:width<768?844:1000});await noOverflow();
        for(const pre of await page.locator('.command-body pre:visible').all())assert(await pre.evaluate(el=>el.scrollWidth<=el.clientWidth+1));
        if(width===390 || width===320) {await page.locator('.command-call[data-result="failed"]').scrollIntoViewIfNeeded();await capture(width===390?'24-detailed-command-mobile':'27-detailed-command-mobile-320');}
      }
      await page.setViewportSize({width:1440,height:1000});await page.locator('[data-action="tool-details"]').click();
      assert.equal(await page.locator('.command-body:visible').count(),0);assert(await page.locator('.tool-attention').isVisible());
      assert.equal(await page.locator('#prompt').inputValue(),draft);await page.locator('#chat-panel').evaluate(el=>el.scrollTop=0);
    });
    await check('侧栏状态只用圆点无文字/额外行，tooltip与无障碍状态保留；切换器仍有文字',async () => {
      const states={local:{design:'已完成',theme:'运行中',architecture:'待授权'},studio:{design:'出错',build:'排队中'}};
      await page.locator('#sidebar-sessions [data-node-group="studio"]>.node-heading').click();
      for(const [node,sessions] of Object.entries(states))for(const [id,label] of Object.entries(sessions)) {
        for(const scope of ['#sidebar-sessions .sidebar-catalog','#switcher-sessions']) {
          const row=page.locator(`${scope} [data-node="${node}"][data-session-id="${id}"]`);
          const status=row.locator('.session-state');assert.equal(await row.getAttribute('aria-description'),label);
          if(scope.startsWith('#sidebar')) {
            assert.equal(await status.textContent(),'');assert.equal(await status.getAttribute('title'),label);assert.equal(await status.locator('svg').count(),0);
            assert.equal(await row.locator('.session-meta').count(),0);
            const style=await status.evaluate(el=>({width:getComputedStyle(el).width,color:getComputedStyle(el).backgroundColor}));assert.equal(style.width,'7px');assert.notEqual(style.color,'rgba(0, 0, 0, 0)');
          } else assert.equal(await status.textContent(),label);
        }
      }
      await capture('21-sidebar-session-states-desktop');
      await page.locator('#sidebar-sessions [data-node-group="studio"]>.node-heading').click();
      await page.setViewportSize({width:390,height:844});await page.locator('.mobile-menu').click();await capture('25-sidebar-session-states-mobile');await page.keyboard.press('Escape');
      await page.setViewportSize({width:1440,height:1000});
    });
    await check('顶栏使用真实浏览器全屏：进入/退出、Escape、外部退出同步，拒绝不伪造成功',async () => {
      const button=page.locator('[data-action="fullscreen"]');assert(await button.isVisible());assert(!await button.isDisabled());
      const waitFullscreen=active=>page.waitForFunction(active=>!!document.fullscreenElement===active && document.querySelector('[data-action="fullscreen"]').getAttribute('aria-pressed')===String(active) && !document.querySelector('[data-action="fullscreen"]').disabled,active);
      await button.click();await waitFullscreen(true);assert.equal(await button.getAttribute('aria-pressed'),'true');
      assert.equal(await button.getAttribute('aria-label'),'退出全屏');assert(await button.evaluate(el=>el===document.activeElement));await capture('26-browser-fullscreen-desktop');
      await button.click();await waitFullscreen(false);assert.equal(await button.getAttribute('aria-pressed'),'false');
      await button.click();await waitFullscreen(true);await page.keyboard.press('Escape');await waitFullscreen(false);
      await button.click();await waitFullscreen(true);await page.locator('#prompt').focus();await page.evaluate(()=>document.exitFullscreen());await waitFullscreen(false);
      assert(await page.locator('#prompt').evaluate(el=>el===document.activeElement));
      assert.equal(await button.getAttribute('aria-label'),'进入全屏');
      await page.evaluate(()=>{document.documentElement.requestFullscreen=()=>Promise.reject(new Error('simulated permission denial'));});
      await button.click();await page.getByText('全屏切换未成功；当前显示状态保持不变。',{exact:true}).waitFor();
      assert.equal(await page.evaluate(()=>!!document.fullscreenElement),false);assert.equal(await button.getAttribute('aria-pressed'),'false');assert(!await button.isDisabled());
      await page.evaluate(()=>{delete document.documentElement.requestFullscreen;document.querySelector('.toast').hidden=true;});
      await page.setViewportSize({width:1440,height:1000});
    });
    await check('黑白灰默认配色；展示方式仅在设置，侧栏保留最近会话与项目树',async () => {
      const accent = await page.locator('.brand>.brand-symbol').evaluate(el => getComputedStyle(el).backgroundColor);assert.equal(accent,'rgb(36, 36, 36)');
      assert.equal(await page.locator('#sidebar-sessions .node-group').count(),2);
      await page.locator('#sidebar-sessions [data-node-group="local"] .node-heading').getByText('本机',{exact:true}).waitFor();
      await page.locator('#sidebar-sessions [data-node-group="studio"] .node-heading').getByText('开发机',{exact:true}).waitFor();
      assert.equal(await page.locator('[data-layout],.session-layout').count(),0);
      assert.equal(await page.locator('#sidebar-sessions .sidebar-recent').count(),1);
      assert.equal(await page.locator('#sidebar-sessions .project-outline').count(),4);
      assert.equal(await page.locator('#sidebar-sessions .sidebar-catalog .session').count(),5);
      const project=page.locator('#sidebar-sessions [data-node-group="local"] .project-outline').first();
      await project.locator('.project-heading').click();assert(!await project.evaluate(el=>el.open));
      await project.locator('.project-heading').click();assert(await project.evaluate(el=>el.open));
      await setSessionDisplay('flat','18-session-display-settings-desktop');
      assert.equal(await page.locator('#sidebar-sessions .node-group').count(),0);
      assert.equal(await page.locator('#sidebar-sessions .project-outline').count(),4);
      assert.equal(await page.locator('#sidebar-sessions .project-node-origin').count(),4);
      await capture('14-sessions-flat-desktop');await setSessionDisplay('grouped');
    });
    await check('没有独立新建/工作台导航；项目文件夹入口保留节点与项目作用域',async () => {
      assert.equal(await page.locator('.sidebar .new-session,.main-nav [data-view="workspace"]').count(),0);
      const draft=await page.locator('#prompt').inputValue();
      const project=page.locator('#sidebar-sessions [data-node-group="local"] .project-outline').first();
      await page.getByRole('button',{name:'在本机的rpi-work中新建会话',exact:true}).click();
      assert(await page.locator('#new-session-dialog').isVisible());assert.equal(await page.locator('.new-session-project').innerText(),'本机 · rpi-work');
      assert(await project.evaluate(el=>el.open));await capture('20-project-folder-entry-desktop');
      await page.keyboard.press('Escape');assert.equal(await page.locator('#prompt').inputValue(),draft);
      await page.locator('#sidebar-sessions [data-node-group="studio"]>.node-heading').click();
      await page.getByRole('button',{name:'在开发机的rpi-work中新建会话',exact:true}).click();assert.equal(await page.locator('.new-session-project').innerText(),'开发机 · rpi-work');
      await page.keyboard.press('Escape');assert.equal(await page.locator('#session-origin').innerText(),'本机 · rpi-work');
      assert.equal(await page.locator('#sidebar-sessions .sidebar-catalog .session').count(),5);
      await page.locator('#sidebar-sessions [data-node-group="studio"]>.node-heading').click();
    });
    await view('resources'); await capture('02-resources-desktop');
    await check('资源分类、搜索与来源切换，来源语义不混淆',async () => {await page.locator('[data-kind="packages"]').click();await capture('30-packages-list-desktop');const row = page.locator('.resource-package').first();const rowBox = await row.boundingBox();assert(rowBox.width > rowBox.height * 2.5,'资源实际以列表行呈现');assert(await row.locator('.resource-meta').isVisible());assert.equal(await row.locator('[data-icon="more"], .package-top').count(),0);assert(await row.getByRole('button',{name:'查看详情',exact:false}).isVisible());await page.setViewportSize({width:390,height:844});await capture('34-resource-list-mobile');assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));await page.setViewportSize({width:1440,height:1000});await page.locator('[data-kind="skills"]').click();assert.equal(await page.locator('.resource-package').count(),3);await capture('31-skills-list-desktop');await page.getByRole('textbox',{name:'搜索资源',exact:true}).fill('Dioxus');assert.equal(await page.locator('.resource-package').count(),1);await page.getByRole('textbox',{name:'搜索资源',exact:true}).fill('不存在的资源');assert(await page.locator('.resource-empty').isVisible());await page.getByRole('textbox',{name:'搜索资源',exact:true}).fill('');await page.locator('[data-kind="extensions"]').click();await capture('32-extensions-list-desktop');await page.locator('[data-source="本地设计"]').click();assert.match(await page.locator('#resource-source-note').innerText(),/不等于当前节点已经安装/);await page.locator('[data-source="发现资源"]').click();assert.match(await page.locator('#resource-source-note').innerText(),/不等于已安装/);await page.locator('[data-source="当前节点"]').click();await page.locator('[data-kind="packages"]').click();
      await page.locator('.resource-package').first().getByRole('button',{name:'查看详情',exact:false}).click();assert(await page.locator('#detail-dialog').isVisible());assert.equal(await page.locator('#detail-resource-source').innerText(),'当前节点');assert.equal(await page.locator('#detail-resource-action').innerText(),'已安装');assert(await page.locator('#detail-resource-action').isDisabled());await page.keyboard.press('Escape');
      await page.locator('[data-source="发现资源"]').click();await page.locator('.resource-package').first().getByRole('button',{name:'查看详情',exact:false}).click();assert.equal(await page.locator('#detail-resource-action').innerText(),'安装到当前节点');assert(!await page.locator('#detail-resource-action').isDisabled());await capture('33-resource-install-detail');await page.locator('#detail-resource-action').click();assert.equal(await page.locator('#detail-resource-action').innerText(),'原型不执行安装');await page.keyboard.press('Escape');
      await page.locator('[data-source="本地设计"]').click();await page.locator('.resource-package').first().getByRole('button',{name:'查看详情',exact:false}).click();assert.equal(await page.locator('#detail-resource-action').innerText(),'导入到本地设计');await page.keyboard.press('Escape');await page.locator('[data-source="当前节点"]').click();await page.locator('[data-kind="packages"]').click();});
    await page.locator('.sidebar [data-view="settings"]').click(); await capture('03-settings-desktop');
    await check('设置页tab点击区域充足；工作协同聚焦节点连接与可信协助边界',async () => {
      await page.getByRole('heading',{name:'界面与显示',exact:true}).waitFor();assert(await page.getByText('仅本设备',{exact:true}).isVisible());
      const tab=page.locator('.settings-tabs button').first();const tabStyle=await tab.evaluate(el=>({height:getComputedStyle(el).minHeight,padding:getComputedStyle(el).padding}));assert.equal(tabStyle.height,'44px');assert.match(tabStyle.padding,/16px/);
      await page.getByRole('button',{name:'工作协同',exact:true}).click();assert(await page.locator('#collaboration-panel').isVisible());assert(!await page.locator('.settings-tabs').isVisible());assert.match(await page.locator('#collaboration-panel').innerText(),/连接其他设备/);assert.match(await page.locator('#collaboration-panel').innerText(),/允许远程协助/);assert.match(await page.locator('#collaboration-panel').innerText(),/允许 Web 访问/);assert.match(await page.locator('#collaboration-panel').innerText(),/协同插件/);await noOverflow();await capture('28-collaboration-settings-desktop');
      await page.setViewportSize({width:390,height:844});await noOverflow();assert(await page.locator('.mobile-connect-actions').isVisible());assert(await page.getByRole('button',{name:'扫码连接',exact:true}).isVisible());assert(await page.getByRole('button',{name:'输入地址',exact:true}).isVisible());assert(!await page.locator('.remote-assist-card').isVisible());assert(!await page.locator('.web-access-card').isVisible());assert(!await page.getByText('允许远程协助',{exact:true}).isVisible());await capture('29-collaboration-settings-mobile');
      await page.setViewportSize({width:1440,height:1000});await page.getByRole('button',{name:'供应商',exact:true}).click();assert(await page.locator('#providers-panel').isVisible());assert.equal(await page.locator('#providers-panel .provider-row').count(),2);assert.match(await page.locator('#providers-panel').innerText(),/当前执行节点/);assert.match(await page.locator('#providers-panel').innerText(),/API Key：已保存/);await page.getByRole('button',{name:'查看模型',exact:true}).first().click();assert(await page.locator('#provider-models-dialog').isVisible());await page.keyboard.press('Escape');await page.getByRole('button',{name:'配置密钥',exact:true}).click();assert(await page.locator('#provider-key-dialog').isVisible());assert.match(await page.locator('#provider-key-dialog').innerText(),/不会验证远端认证/);await page.keyboard.press('Escape');await page.getByRole('button',{name:'添加供应商',exact:true}).click();assert(await page.locator('#provider-definition-dialog').isVisible());assert.match(await page.locator('#provider-definition-dialog').innerText(),/不会请求 Base URL/);assert(await page.getByLabel('供应商 ID',{exact:true}).isVisible());assert(await page.locator('#provider-definition-dialog').getByText('从当前会话获取模型列表',{exact:true}).isVisible());await page.evaluate(() => { document.querySelector('.toast').hidden = true; });await capture('36-provider-add-desktop');await page.setViewportSize({width:390,height:844});await capture('36-provider-add-mobile');await page.keyboard.press('Escape');await page.setViewportSize({width:1440,height:1000});await page.setViewportSize({width:390,height:844});await noOverflow();await page.evaluate(() => { document.querySelector('.toast').hidden = true; });await capture('35-provider-settings-mobile');await page.setViewportSize({width:1440,height:1000});await capture('35-provider-settings-desktop');await page.getByRole('button',{name:'界面与显示',exact:true}).click();assert(await page.locator('.settings-tabs').isVisible());assert(await page.locator('#appearance-panel').isVisible());await page.setViewportSize({width:1440,height:1000});await noOverflow();
    });
    await view('workspace');await page.locator('#prompt').fill('');
    await check('1440/1280/1024/768/390/320 无水平溢出',async () => {for(const width of [1440,1280,1024,768,390,320]) {await page.setViewportSize({width,height:width<768?844:1000});await noOverflow();const box = await page.locator('.composer').boundingBox();assert(box.x>=0 && box.x+box.width<=width+1);assert(box.y+box.height<=(width<768?844:1000));if(width===390)await capture('05-workspace-mobile');if(width===320)await capture('06-workspace-mobile-320');}});
    await page.setViewportSize({width:390,height:844});
    await check('手机导航与上下文按需打开，Escape 关闭并恢复焦点',async () => {
      await page.locator('.project-files-trigger').click();assert(await page.locator('#project-files-panel').isVisible());await capture('37-project-files-mobile');await page.locator('#project-files-panel .project-changes .project-file-row.changed').first().click();assert(await page.locator('#project-file-diff').isVisible());await capture('38-project-file-diff-sidebar-mobile');await page.locator('#project-file-diff [data-action="project-file-diff-back"]').click();await page.locator('#project-files-panel [data-action="project-files"]').click();await page.keyboard.press('Escape');
      await page.getByRole('button',{name:'打开导航',exact:true}).click();assert(await page.locator('.sidebar').isVisible());await page.keyboard.press('Escape');assert(!await page.locator('.sidebar').isVisible());
      await page.getByRole('button',{name:'任务上下文',exact:true}).click();assert(await page.getByRole('dialog',{name:'任务上下文',exact:true}).isVisible());await capture('08-context-on-demand');
      await page.keyboard.press('Escape');assert(!await page.getByRole('dialog',{name:'任务上下文',exact:true}).isVisible());
      assert(await page.getByRole('button',{name:'任务上下文',exact:true}).evaluate(el => el === document.activeElement));
      await page.getByRole('button',{name:'附件与更多',exact:true}).click();await capture('09-configuration-on-demand');await page.keyboard.press('Escape');
      assert(await page.getByRole('button',{name:'附件与更多',exact:true}).evaluate(el => el === document.activeElement));
    });
    await check('手机顶栏直达会话；遵循设置无排列tab，跨节点同ID草稿不串用',async () => {
      await page.locator('#prompt').fill('本机设计草稿');
      await page.getByRole('button',{name:'切换会话',exact:true}).click();assert(!await page.locator('.sidebar').isVisible());
      assert(await page.locator('#session-dialog').isVisible());assert.equal(await page.locator('#switcher-sessions .node-group').count(),2);await capture('12-session-switcher-grouped-mobile');
      assert.equal(await page.locator('#session-dialog [data-layout],#session-dialog .session-layout').count(),0);
      await page.keyboard.press('Escape');
      await setSessionDisplay('flat','19-session-display-settings-mobile');
      await page.getByRole('button',{name:'切换会话',exact:true}).click();
      assert.equal(await page.locator('#switcher-sessions .flat-session').count(),5);await capture('13-session-switcher-flat-mobile');
      await page.locator('#session-search').fill('开发机');assert.equal(await page.locator('#switcher-sessions .session').count(),2);
      await page.locator('#session-search').fill('不存在的会话');assert(await page.locator('#switcher-sessions .session-empty').isVisible());
      await page.locator('#session-search').fill('');await page.keyboard.press('Escape');
      await setSessionDisplay('grouped');await page.getByRole('button',{name:'切换会话',exact:true}).click();
      await page.locator('#switcher-sessions [data-node="studio"][data-session-id="design"]').click();
      assert.equal(await page.locator('#session-title').innerText(),'验证手机端布局');assert.equal(await page.locator('#session-origin').innerText(),'开发机 · rpi-work');
      assert.equal(await page.locator('#prompt').inputValue(),'');assert.equal(await page.locator('.model-name').innerText(),'GPT-5');
      await page.locator('#prompt').fill('开发机设计草稿');
      await page.getByRole('button',{name:'切换会话',exact:true}).click();await page.locator('#switcher-sessions [data-node="local"][data-session-id="design"]').click();
      assert.equal(await page.locator('#prompt').inputValue(),'本机设计草稿');assert.equal(await page.locator('.model-name').innerText(),'DeepSeek V4 Pro');
      await page.getByRole('button',{name:'切换会话',exact:true}).click();await page.locator('#switcher-sessions [data-node="studio"][data-session-id="design"]').click();assert.equal(await page.locator('#prompt').inputValue(),'开发机设计草稿');
      await page.getByRole('button',{name:'切换会话',exact:true}).click();await page.locator('#switcher-sessions [data-node="local"][data-session-id="design"]').click();await page.locator('#prompt').fill('');
    });
    await check('320/390px 常用选择器与会话面板可操作，无弹层横向溢出',async () => {
      for(const width of [390,320]) {
        await page.setViewportSize({width,height:844});
        for(const [label,dialogId] of [['切换会话','session-dialog'],['切换模型','model-dialog'],['切换思考等级','thinking-dialog'],['附件与更多','configuration-dialog']]) {
          await page.getByRole('button',{name:label,exact:true}).click();
          const dialog=page.locator('#'+dialogId);assert(await dialog.isVisible());
          assert(await dialog.evaluate(el => el.scrollWidth<=el.clientWidth+1));
          const box=await dialog.boundingBox();assert(box.x>=0 && box.x+box.width<=width+1);assert(box.y>=0 && box.y+box.height<=845);
          if(width===390 && dialogId==='model-dialog')await capture('15-model-picker-mobile');
          if(width===390 && dialogId==='thinking-dialog')await capture('16-thinking-picker-mobile');
          if(width===320 && dialogId==='session-dialog')await capture('17-session-switcher-320');
          await page.keyboard.press('Escape');assert(!await dialog.isVisible());
          assert(await page.getByRole('button',{name:label,exact:true}).evaluate(el => el===document.activeElement));
        }
      }
    });
    await check('工作区优先：默认无右栏，正文占高，常用控件常驻，桌面导航可收起',async () => {
      for(const [width,height] of [[1440,1000],[1280,800],[1024,768],[768,844],[390,844],[320,844],[390,500],[844,390]]) {
        await page.setViewportSize({width,height});await noOverflow();
        const body = await page.locator('#chat-panel').boundingBox();
        workspaceGeometry.push({width,height,transcriptHeight:body.height,transcriptViewportRatio:body.height/height});
        assert(body.height/height >= (height<600?0.59:0.70),`Reading area at ${width}x${height}: ${body.height/height}`);
        assert(!await page.locator('#context-dialog').isVisible());
        const composer = await page.locator('.composer').boundingBox();assert(composer.y+composer.height<=height);assert(composer.x>=0 && composer.x+composer.width<=width+1);
        assert.equal(await page.locator('.composer button:visible').count(),4);
        for(const control of ['.composer .attach','.composer .model-button','.composer .thinking-button','.composer .send-button']) {const box=await page.locator(control).boundingBox();assert(box.width>=40 && box.height>=40);}
        if(width===1280)await capture('10-workspace-desktop-1280');
        if(height===500)await capture('11-workspace-short-mobile');
      }
      await page.setViewportSize({width:1440,height:1000});const before=await page.locator('.conversation-card').boundingBox();
      await page.getByRole('button',{name:'打开导航',exact:true}).click();assert(!await page.locator('.sidebar').isVisible());
      const after=await page.locator('.conversation-card').boundingBox();assert(after.width-before.width>=200);await capture('07-workspace-focus');
      await page.getByRole('button',{name:'打开导航',exact:true}).click();
    });
    await check('长会话标题与200%文字下不挤出主操作',async () => {
      await page.setViewportSize({width:320,height:844});
      const original = await page.locator('#session-title').innerText();
      await page.locator('#session-title').evaluate(el => {el.textContent='这是一个很长的中文会话标题用于确认工作区不会被挤出边界';});await noOverflow();
      await page.locator('#session-title').evaluate((el,text) => {el.textContent=text;},original);
      await page.addStyleTag({content:'.workspace-identity h1{font-size:26px!important}.assistant-content p{font-size:30px!important}'});
      await noOverflow();const send=await page.locator('.send-button').boundingBox();assert(send.x+send.width<=320);await page.locator('style').last().evaluate(el=>el.remove());
    });
    await check('资源和设置页面在六档宽度均无内容横向溢出',async () => {
      for(const width of [1440,1280,1024,768,390,320]) {
        await page.setViewportSize({width,height:width<768?844:1000});
        for(const name of ['resources','settings']) {await navigate(name);await noOverflow();}
      }
    });
    await check('快捷搜索可筛选示例会话',async () => {
      await page.keyboard.press('Control+k');
      await page.getByRole('textbox',{name:'搜索节点、项目与会话',exact:true}).fill('主题');
      assert.equal(await page.locator('#switcher-sessions .session').count(),1);
      await page.getByRole('button',{name:'关闭会话切换',exact:true}).click();
    });
    await check('原型无真实网络、无脚本异常',async () => {assert.deepEqual(errors,[]);assert.deepEqual(network,[]);});
    fs.writeFileSync(path.join(__dirname,'verification.json'),JSON.stringify({scope:'standalone static concept only; not actual Dioxus application; short viewport is not real OS keyboard acceptance',revision:'settings-collaboration',checks,workspaceGeometry,errors,networkRequests:network},null,2)+'\n');
  } finally {await browser.close();}
})().catch(error => {console.error(error);process.exitCode=1;});
