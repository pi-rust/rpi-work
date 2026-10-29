const assert=require('node:assert/strict'),fs=require('node:fs');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
(async()=>{
 const node=await startFixture(),browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
 const context=await browser.newContext({viewport:{width:1440,height:1000}}),page=await context.newPage(),results=[],errors=[];
 page.setDefaultTimeout(8000);page.on('pageerror',e=>errors.push(e.message));
 await context.route('https://api.github.com/**',route=>route.fulfill({status:404,body:'{}'}));
 const pass=name=>{results.push(name);console.log('PASS',name)};
 const row=id=>page.locator(`.project-entry[data-project-id="${id}"]`);
 const session=id=>page.locator(`.session-entry[data-session-id="${id}"]`);
 const menu=()=>page.getByRole('menu',{name:'会话操作',exact:true});
 const openSessionMenu=async id=>{await session(id).locator('.session-row-menu').click();await menu().waitFor()};
 const message=()=>page.locator('.workspace-notice');
 try{
  await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4180/');
  await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
  await page.getByRole('button',{name:'连接其他节点',exact:true}).click();await page.getByLabel('节点地址',{exact:true}).fill(node.url);await page.getByRole('button',{name:'连接节点',exact:true}).click();await page.locator('.server-details [role=status]').filter({hasText:'在线'}).waitFor();await page.getByRole('button',{name:'返回会话',exact:true}).click();
  await row('rpi-work').locator('.project-row').click();await session('dark').locator('.session-item').click();
  await row('rpi-work').locator('.project-row-menu').click();await page.getByRole('menuitem',{name:'重命名项目',exact:true}).click();
  const projectName=page.getByRole('dialog',{name:'项目名称与目录',exact:true});await projectName.getByLabel('显示名称',{exact:true}).fill('我的工作台');node.controls.failProjectRename=true;await projectName.getByLabel('显示名称',{exact:true}).press('Enter');await projectName.getByRole('alert').waitFor();assert.equal(await projectName.getByLabel('显示名称',{exact:true}).inputValue(),'我的工作台');node.controls.failProjectRename=false;await projectName.getByLabel('显示名称',{exact:true}).press('Enter');await projectName.waitFor({state:'hidden'});
  assert.equal(node.requests.find(r=>r.path==='/api/v1/projects/rpi-work/rename').body.name,'我的工作台');
  assert.equal(node.requests.filter(r=>r.path==='/api/v1/projects'&&r.method==='POST').length,0);
  await row('rpi-work').getByRole('button',{name:'在 我的工作台 中新建会话',exact:true}).waitFor();
  pass('project rename persists through the node without creating or moving a directory');

  await openSessionMenu('architecture');await menu().getByRole('menuitem',{name:'重命名',exact:true}).click();
  const rename=page.getByRole('dialog',{name:'重命名会话',exact:true});await rename.getByLabel('会话名称',{exact:true}).fill('设计会话');node.controls.failRenameCount=1;await rename.getByRole('button',{name:'保存',exact:true}).click();await rename.getByRole('alert').waitFor();assert.equal(await rename.getByLabel('会话名称',{exact:true}).inputValue(),'设计会话');await rename.getByLabel('会话名称',{exact:true}).press('Enter');await rename.waitFor({state:'hidden'});await session('architecture').getByText('设计会话',{exact:true}).waitFor();
  await page.locator('.app-header h1').filter({hasText:'为工作台添加深色模式'}).waitFor();
  pass('renaming an inactive session targets that row, preserves input on failure, and supports Enter');
  await openSessionMenu('architecture');await page.keyboard.press('ArrowDown');await page.keyboard.press('Escape');await menu().waitFor({state:'hidden'});
  await openSessionMenu('architecture');await menu().getByRole('menuitem',{name:'置顶',exact:true}).click();await session('architecture').getByText('置顶',{exact:true}).waitFor();assert.equal(await row('rpi-work').locator('.session-entry').first().getAttribute('data-session-id'),'architecture');
  pass('session menus support Escape and keyboard navigation; pinning updates node metadata and ordering');

  node.controls.failMetadata=true;await openSessionMenu('architecture');await menu().getByRole('menuitem',{name:'归档会话',exact:true}).click();await message().filter({hasText:'操作失败'}).waitFor();assert(await session('architecture').isVisible());node.controls.failMetadata=false;
  await openSessionMenu('architecture');await menu().getByRole('menuitem',{name:'归档会话',exact:true}).click();await session('architecture').waitFor({state:'hidden'});await message().getByRole('button',{name:'撤销',exact:true}).click();await session('architecture').waitFor();
  await openSessionMenu('architecture');await menu().getByRole('menuitem',{name:'归档会话',exact:true}).click();await session('architecture').waitFor({state:'hidden'});
  await page.reload();await row('rpi-work').locator('.project-row').click();assert.equal(await session('architecture').count(),0);
  await page.getByRole('button',{name:'已归档',exact:true}).click();const archive=page.getByRole('dialog',{name:'已归档',exact:true});await archive.getByRole('button',{name:'恢复会话 设计会话',exact:true}).click();await archive.getByText('暂无已归档会话。',{exact:true}).waitFor();await page.keyboard.press('Escape');await session('architecture').waitFor();
  pass('session archive is confirmed by the node, survives reload, supports undo and restoration, and retains failures');

  await row('pi-rust').locator('.project-row-menu').click();await page.getByRole('menuitem',{name:'归档项目',exact:true}).click();await row('pi-rust').waitFor({state:'hidden'});
  await page.getByRole('button',{name:'已归档',exact:true}).click();await archive.getByRole('button',{name:'恢复项目 pi-rust',exact:true}).waitFor();await page.keyboard.press('Escape');await page.reload();assert.equal(await row('pi-rust').count(),0);
  await page.getByRole('button',{name:'已归档',exact:true}).click();await archive.getByRole('button',{name:'恢复项目 pi-rust',exact:true}).click();await page.keyboard.press('Escape');await row('pi-rust').waitFor();
  pass('project sidebar archives persist locally and restore without deleting project or session data');

  await row('rpi-work').locator('.project-row').click();await session('dark').locator('.session-item').click();const composer=page.getByRole('textbox',{name:'发送给 agent 的消息',exact:true});await composer.fill('保留在原会话的草稿');await page.getByRole('button',{name:'从这条消息新建分支',exact:true}).first().click();await page.locator('.app-header h1').filter({hasText:'分支'}).waitFor();
  assert.equal(node.requests.filter(r=>r.path.endsWith('/fork')).at(-1).body.entryId,'user-dark');assert.equal(await composer.inputValue(),'');
  await session('dark').locator('.session-item').click();assert.equal(await composer.inputValue(),'保留在原会话的草稿');
  pass('fork from a canonical message opens a new branch and retains the source conversation draft');
  await page.locator('.app-header .session-row-menu').click();await menu().getByRole('menuitem',{name:'复制会话（Clone）',exact:true}).click();await page.locator('.app-header h1').filter({hasText:'副本'}).waitFor();
  node.controls.failFork=true;await page.locator('.app-header .session-row-menu').click();await menu().getByRole('menuitem',{name:'新建分支（Fork）',exact:true}).click();await message().filter({hasText:'操作失败'}).waitFor();await page.locator('.app-header h1').filter({hasText:'副本'}).waitFor();node.controls.failFork=false;
  pass('clone opens the resulting conversation; a failed fork leaves its source selected');
  for(const width of [390,320]){await page.setViewportSize({width,height:844});await page.locator('.app-header .session-row-menu').click();await menu().waitFor();const box=await menu().boundingBox();assert(box.x>=0&&box.x+box.width<=width+1);assert(box.y>=0&&box.y+box.height<=844+1);await page.screenshot({path:`artifacts/workspace-menu-${width}.png`,animations:'disabled'});await page.keyboard.press('Escape');}
  assert.equal(node.requests.filter(r=>r.method==='DELETE').length,0);assert.deepEqual(errors,[]);
  pass('390/320 menus stay on screen; all archive and fork operations avoid DELETE and console errors');
 }catch(error){errors.push(error.message);await page.screenshot({path:'artifacts/workspace-management-failure.png',animations:'disabled'});throw error}
 finally{fs.writeFileSync('artifacts/workspace-management-validation.json',JSON.stringify({results,errors},null,2));await browser.close();await node.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
