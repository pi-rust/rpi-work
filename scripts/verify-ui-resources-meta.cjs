const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
(async()=>{
 const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'}),page=await browser.newPage({viewport:{width:1440,height:1000}});page.setDefaultTimeout(6500);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 const sidebar=()=>page.locator('.sidebar');
 try{
  await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4173/target/parity-web/');
  await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
  await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);await page.getByRole('button',{name:'连接节点',exact:true}).click();await page.getByRole('status').filter({hasText:'在线'}).waitFor();await page.getByRole('button',{name:'返回会话',exact:true}).click();
  await page.locator('.project-row').filter({hasText:'rpi-work'}).click();await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
  assert.equal(await sidebar().getByRole('button',{name:'元Agent',exact:true}).count(),0);
  await sidebar().getByRole('button',{name:'Packages',exact:true}).click();await page.getByRole('button',{name:/导入资源包/}).click();
  const imported={schemaVersion:1,kind:'rpi-work-design-package',id:'review.full-app',name:'正式应用审查包',version:'1.2.3',author:'QA',description:'通过正式入口导入',resources:{skills:[{id:'review',name:'完整应用Skill',description:'明确引用',content:'# Skill上下文\n检查完整应用。'}],tools:['read'],prompts:[{id:'p',name:'审查模板',kind:'template',content:'请审查改动。'}]},design:{resourceKind:'Package',sources:{'README.md':'immutable full app source'}}};
  const importDialog=page.getByRole('dialog',{name:'导入设计包',exact:true});await importDialog.locator('textarea').fill(JSON.stringify(imported));await importDialog.getByRole('button',{name:'校验并导入',exact:true}).click();await page.locator('.resource-card').filter({hasText:'正式应用审查包'}).waitFor();
  pass('official Packages navigation imports a complete local design package');
  await sidebar().getByRole('button',{name:'技能库',exact:true}).click();await page.locator('.resource-card').filter({hasText:'完整应用Skill'}).waitFor();
  await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
  await page.getByRole('button',{name:'打开会话资源配置',exact:true}).click();const config=page.getByRole('dialog',{name:'会话资源配置',exact:true});
  const sessionCard=config.locator('.session-resource-row').filter({hasText:'正式应用审查包'});await sessionCard.getByRole('button',{name:'用于本会话',exact:true}).click();assert.match(await sessionCard.innerText(),/通过正式入口导入/);assert.match(await sessionCard.innerText(),/待节点应用/);await sessionCard.getByRole('button',{name:'查看内容',exact:true}).click();const detail=page.getByRole('dialog',{name:'资源详情',exact:true});await detail.waitFor();await detail.getByText('完整应用Skill · review',{exact:true}).waitFor();await page.keyboard.press('Escape');await detail.waitFor({state:'hidden'});assert(await config.isVisible());pass('session package cards show real description and pending binding; nested full details close without losing configuration');
  for(const width of [1440,390,320]){
   await page.setViewportSize({width,height:width===1440?1000:844});await sessionCard.scrollIntoViewIfNeeded();const bounds=await sessionCard.boundingBox();let bottom=bounds.y;
   for(const selector of ['.resource-card-title',':scope > p','.resource-summary',':scope > small','.resource-steps','.resource-actions']){const region=await sessionCard.locator(selector).first().boundingBox();assert(region,`${width}: missing ${selector}`);assert(region.y>=bottom-1,`${width}: overlapping card section ${selector}`);assert(region.x>=bounds.x-1&&region.x+region.width<=bounds.x+bounds.width+1,`${width}: overflowing card section ${selector}`);bottom=region.y+region.height}
   const heading=await sessionCard.locator('h3').boundingBox();assert(heading.width>=Math.min(140,bounds.width*.4),`${width}: card heading collapsed into vertical letters`);assert(await config.evaluate(e=>e.scrollWidth<=e.clientWidth+1));await page.screenshot({path:`artifacts/ui-session-packages-${width}.png`,animations:'disabled'});
   const intro=config.locator('.session-resource-intro'),introText=await intro.locator('p').boundingBox(),introButton=intro.getByRole('button');if(await introButton.count()){const button=await introButton.boundingBox();assert(introText.y+introText.height<=button.y+1||button.y+button.height<=introText.y+1||introText.x+introText.width<=button.x+1||button.x+button.width<=introText.x+1,`${width}: package intro and browse action overlap`);assert(await introButton.evaluate(e=>e.scrollWidth<=e.clientWidth+1),`${width}: browse action text overflows its button`)}
  }await page.setViewportSize({width:1440,height:1000});pass('package card title, description, counts, source, steps and actions stack without overlap or clipping at desktop and 390/320');
  await config.getByRole('button',{name:'Tools',exact:true}).click();await config.getByRole('checkbox',{name:'read',exact:true}).waitFor();
  assert(fixture.requests.some(r=>r.body.type==='get_available_tools'));await page.keyboard.press('Escape');
  const toolbar=await page.locator('.section-tabs').boundingBox(),transcript=await page.locator('.transcript').boundingBox();assert.equal(toolbar.height,44);assert.equal(transcript.y,toolbar.y+44);
  pass('top resource configuration binds version and reads real node tools without another toolbar row');
  const prompt=page.locator('.composer-main textarea');await prompt.fill('只发送用户输入');await page.getByRole('button',{name:'发送消息',exact:true}).click();
  const request=fixture.requests.findLast(r=>r.body.type==='prompt');assert(request);
  await page.getByRole('button',{name:'添加项目',exact:true}).click();const project=page.getByRole('dialog',{name:'添加项目',exact:true});
  if(await project.locator('#project-picker-path').count()){
   await project.locator('#project-picker-path').fill('D:/Projects/review-meta');await project.locator('#project-picker-name').fill('我的元Agent项目');await project.getByRole('radio',{name:/元Agent/}).check();await project.getByRole('button',{name:'打开项目',exact:true}).click();
  }else{
   await project.getByLabel('目录路径',{exact:true}).fill('D:/Projects/review-meta');await project.getByLabel('显示名称',{exact:true}).fill('我的元Agent项目');await project.getByLabel('项目用途',{exact:true}).selectOption('元Agent');await project.getByRole('button',{name:'添加项目',exact:true}).click();
  }
  await page.locator('.project-row').filter({hasText:'我的元Agent项目'}).waitFor();await page.locator('.project-row').filter({hasText:'我的元Agent项目'}).click();await sidebar().getByRole('button',{name:'元Agent',exact:true}).click();
  await page.getByRole('button',{name:'+ 新建能力',exact:true}).click();await page.getByLabel('名称',{exact:true}).fill('完整应用能力');await page.getByLabel('资源 ID',{exact:true}).fill('personal.full-app');await page.getByRole('button',{name:'创建设计草稿',exact:true}).click();await page.getByRole('heading',{name:'完整应用能力',exact:true}).waitFor();
  pass('MetaAgent is available through project purpose and creates a personal resource');
  await page.getByRole('button',{name:'开发会话',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.composer-main textarea')?.value.includes('完整所选源码快照'));
  const context=await prompt.inputValue();assert.match(context,/package\.json/);assert.match(context,/README\.md/);assert.equal(fixture.requests.filter(r=>r.body.type==='prompt').length,1);
  await page.getByRole('button',{name:'回到元Agent源稿',exact:true}).click();await page.getByRole('heading',{name:'完整应用能力',exact:true}).waitFor();
  pass('development opens a real ordinary session with source snapshot draft and explicit return');
  await page.getByRole('button',{name:'结构校验',exact:true}).click();await page.waitForFunction(()=>[...document.querySelectorAll('button')].some(b=>b.textContent==='创建测试快照'&&!b.disabled));await page.getByRole('button',{name:'创建测试快照',exact:true}).click();await page.waitForFunction(()=>[...document.querySelectorAll('button')].some(b=>b.textContent==='发布个人版本'&&!b.disabled));await page.getByRole('button',{name:'发布个人版本',exact:true}).click();await page.getByRole('button',{name:'继续迭代 · bump patch',exact:true}).waitFor();
  await sidebar().getByRole('button',{name:'Packages',exact:true}).click();await page.locator('.resource-card').filter({hasText:'完整应用能力'}).waitFor();
  pass('published MetaAgent version is acknowledged by the same formal resource library');
  assert.equal(request.body.message,'只发送用户输入');pass('desired resource metadata does not silently alter user prompt');
  assert.deepEqual(errors,[]);pass('no application JavaScript errors');fs.writeFileSync('artifacts/ui-resources-meta-validation.json',JSON.stringify({results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-resources-meta-failure.png'});fs.writeFileSync('artifacts/ui-resources-meta-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
