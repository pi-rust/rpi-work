const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
(async()=>{
 const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
 const page=await browser.newPage({viewport:{width:1200,height:950}});page.setDefaultTimeout(6000);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 try{
  await page.goto('http://127.0.0.1:4182/modules/project/');await page.getByRole('button',{name:'projects',exact:true}).click();
  await page.getByLabel('Fixture node URL').fill(fixture.url);await page.getByRole('button',{name:'Connect fixture',exact:true}).click();
  await page.getByRole('button',{name:'Open project picker',exact:true}).click();
  const picker=page.getByRole('dialog',{name:'添加项目',exact:true});
  const dir=picker.locator('#project-picker-path'),name=picker.locator('#project-picker-name');
  await dir.fill('D:\\Projects\\review-one');await page.waitForFunction(()=>document.querySelector('#project-picker-name')?.value==='review-one');
  await name.fill('我的自定义名称');await dir.fill('D:/Projects/review-two');assert.equal(await name.inputValue(),'我的自定义名称');
  await picker.getByRole('radio',{name:/元Agent/}).check();await picker.getByRole('button',{name:'打开项目',exact:true}).click();
  await page.waitForFunction(()=>[...document.querySelectorAll('.module-notice')].some(e=>e.textContent.includes('review-two')));
  const intent=JSON.parse(await page.locator('pre.module-notice').innerText());assert.deepEqual(intent,{path:'D:/Projects/review-two',name:'我的自定义名称',meta:true,create:false,initGit:false});
  pass('existing-directory flow normalizes path, retains custom alias and emits MetaAgent purpose');
  await dir.fill('d:/projects/RPI-WORK/');await picker.locator('.project-existing-note').waitFor();assert(await picker.getByRole('radio',{name:/元Agent/}).isDisabled());
  await picker.getByRole('button',{name:'新建目录',exact:true}).click();await page.getByRole('button',{name:'创建项目',exact:true}).click();await page.getByRole('alert').filter({hasText:/已经登记/}).waitFor();
  pass('case-insensitive Windows duplicate cannot be created as a second directory');
  await page.getByRole('button',{name:'打开已有目录',exact:true}).click();await dir.fill('D:/Projects');
  await page.getByRole('button',{name:'浏览…',exact:true}).click();const folders=page.getByRole('dialog',{name:'选择项目文件夹',exact:true});
  await folders.locator('.project-folder-row').filter({hasText:'pi-rust'}).waitFor();
  assert(fixture.requests.some(r=>r.path.startsWith('/api/v1/project-folders')));
  await page.keyboard.press('Escape');await folders.waitFor({state:'hidden'});assert(await picker.isVisible());
  pass('remote node folders load and nested Escape retains project form');
  await picker.getByRole('button',{name:'浏览…',exact:true}).click();
  const row=folders.locator('.project-folder-row').filter({hasText:'pi-rust'});
  await row.getByRole('button',{name:'进入此目录',exact:true}).click();
  await folders.locator('.project-folder-toolbar code').filter({hasText:'D:/Projects/pi-rust'}).waitFor();
  assert(fixture.requests.some(r=>r.path==='/api/v1/project-folders'&&r.query.includes('pi-rust')));
  await folders.getByRole('button',{name:'使用此目录',exact:true}).click();
  assert.equal(await dir.inputValue(),'D:/Projects/pi-rust');assert.equal(await name.inputValue(),'我的自定义名称');
  pass('entering a remote directory requests its children and keeps custom alias');
  for(const width of [390,320]){await page.setViewportSize({width,height:844});await page.screenshot({path:`artifacts/ui-project-module-${width}.png`});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));const box=await picker.boundingBox();assert(box.x>=0&&box.x+box.width<=width+1);}
  pass('390/320 project form remains bounded');
  await page.keyboard.press('Escape');await page.getByRole('button',{name:'Global search',exact:true}).click();const search=page.getByRole('dialog',{name:'找到你的工作',exact:true});await search.getByLabel('搜索项目、目录或会话').fill('独立任务A');assert.equal(await search.locator('.search-result-row').count(),1);await search.getByRole('button',{name:/独立任务A/}).click();await page.waitForFunction(()=>document.querySelector('pre.module-notice')?.textContent==='draft-a');
  pass('global search uses independent query and emits actual session id');
  assert.deepEqual(errors,[]);pass('no application JavaScript errors');
  fs.writeFileSync('artifacts/project-module-validation.json',JSON.stringify({scope:'test-only module harness; parent CRUD integration required',results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-project-module-failure.png'});fs.writeFileSync('artifacts/project-module-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
