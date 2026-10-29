// Diagnostic visual inventory, not a replacement for the final acceptance suite.
const fs=require('node:fs'),path=require('node:path');
const {chromium}=require('../../tests/support/browser.cjs');
const {startFixture}=require('../../tests/fixtures/ui-fixture.cjs');
async function connect(page,node){await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();await page.getByRole('button',{name:'连接其他节点',exact:true}).click();await page.getByLabel('节点地址',{exact:true}).fill(node.url);await page.getByRole('button',{name:'连接节点',exact:true}).click();await page.getByRole('status').filter({hasText:'在线'}).waitFor();await page.getByRole('button',{name:'返回会话',exact:true}).click();await page.locator('.project-row').filter({hasText:'rpi-work'}).click();await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).waitFor()}
async function nav(page,name){if(page.viewportSize().width<701)await page.getByRole('button',{name:'打开项目导航',exact:true}).click();await page.locator('.sidebar').getByRole('button',{name,exact:true}).click()}
async function providers(page,connection=false){await nav(page,'打开设置');await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'Providers',exact:true}).click();await page.locator('.provider-list').getByRole('button',{name:'用于此会话',exact:true}).waitFor();if(connection)await page.locator('.provider-list').getByRole('button',{name:/^连接设置(?:：.+)?$/,exact:true}).first().click()}
async function devices(page,mode='pc'){
 await nav(page,'打开设置');await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:page.viewportSize().width<701?'连接我的电脑':'执行节点',exact:true}).click();
 if(mode==='qr'){await page.getByRole('button',{name:'手机连接这台电脑',exact:true}).click();const dialog=page.getByRole('dialog',{name:'手机连接这台电脑',exact:true});await dialog.getByLabel('手机可访问的节点地址',{exact:true}).fill('https://192.168.50.8:3030');await dialog.locator('#device-address-qr-code svg').waitFor()}
 if(mode==='advanced'){await page.getByRole('button',{name:'手动输入地址',exact:true}).click();await page.getByLabel('电脑节点地址',{exact:true}).fill('https://work.example.com:3030');await page.locator('#phone-manual-connection').scrollIntoViewIfNeeded()}
}

async function metaActual(page,editor=false){
 if(page.viewportSize().width<701)await page.getByRole('button',{name:'打开项目导航',exact:true}).click();await page.getByRole('button',{name:'添加项目',exact:true}).click();const dialog=page.getByRole('dialog',{name:'添加项目',exact:true});await dialog.locator('#project-picker-path').fill(`D:/Projects/visual-meta-${page.viewportSize().width}${editor?'-editor':''}`);await dialog.locator('#project-picker-name').fill('视觉审查元Agent');await dialog.getByRole('radio',{name:/元Agent/}).check();await dialog.getByRole('button',{name:'打开项目',exact:true}).click();await nav(page,'元Agent');await page.getByRole('button',{name:'+ 新建能力',exact:true}).click();await page.getByLabel('名称',{exact:true}).fill('视觉审查包');await page.getByLabel('资源 ID',{exact:true}).fill(`personal.visual-review-${page.viewportSize().width}${editor?'-editor':''}`);await page.getByRole('button',{name:'创建设计草稿',exact:true}).click();if(editor)await page.getByRole('button',{name:'编辑源码',exact:true}).click();
}
async function metaReference(page,editor=false){
 await page.evaluate(()=>addProjectForm());await page.locator('input[name="project-type"][value="capability"]').check();await page.locator('#project-name').fill('视觉审查元Agent');await page.locator('#project-path').fill('D:/Projects/visual-meta');await page.locator('[data-meta-action="create-project"]').click();await page.evaluate(({width,editor})=>{createMetaResource(activeProjectProfile(),'package','视觉审查包',`personal.visual-review-${width}${editor?'-editor':''}`);navigate('meta-agent');if(editor)metaOpenEditor()},{width:page.viewportSize().width,editor});
}
const cases=[
 ['packages',p=>nav(p,'Packages'),p=>p.evaluate(()=>navigate('packages'))],
 ['skills',p=>nav(p,'技能库'),p=>p.evaluate(()=>navigate('skills'))],
 ['extensions',p=>nav(p,'扩展'),p=>p.evaluate(()=>navigate('extensions'))],
 ['settings',p=>nav(p,'打开设置'),p=>p.evaluate(()=>navigate('settings'))],
 ['providers',p=>providers(p),p=>p.evaluate(()=>navigate('providers'))],
 ['provider-connection',p=>providers(p,true),p=>p.evaluate(()=>providerConfig('DeepSeek'))],
 ['devices',p=>devices(p),p=>p.evaluate(()=>navigate('devices'))],
 ['devices-phone',p=>devices(p,'phone'),p=>p.evaluate(()=>openMobileConnect('scan'))],
 ['devices-advanced',p=>devices(p,'advanced'),p=>p.evaluate(()=>openMobileConnect('advanced'))],
 ['devices-qr',p=>devices(p,'qr'),p=>p.evaluate(()=>openPairing())],
 ['session-resources',p=>p.getByRole('button',{name:'打开会话资源配置',exact:true}).click(),p=>p.evaluate(()=>sessionResources('packages'))],
 ['session-skills',p=>p.getByRole('button',{name:'打开会话Skills配置',exact:true}).click(),p=>p.evaluate(()=>sessionResources('skills'))],
 ['session-prompts',p=>p.getByRole('button',{name:'打开会话提示词配置',exact:true}).click(),p=>p.evaluate(()=>sessionResources('prompts'))],
 ['tools',async p=>{await p.getByRole('button',{name:'打开会话资源配置',exact:true}).click();await p.getByRole('dialog').getByRole('button',{name:'Tools',exact:true}).click();await p.getByRole('dialog').getByRole('checkbox',{name:'read',exact:true}).waitFor()},p=>p.evaluate(()=>sessionResources('tools'))],
 ['stats',p=>p.locator('.runtime-footer .cache-metric').click(),p=>p.evaluate(()=>statsModal())],
 ['context',p=>p.locator('.runtime-footer .context-meter').click(),p=>p.evaluate(()=>contextModal())],
 ['theme',p=>p.getByRole('button',{name:'外观与主题',exact:true}).click(),p=>p.evaluate(()=>appearanceSettingsModal())],
 ['project-add',async p=>{if(p.viewportSize().width<701)await p.getByRole('button',{name:'打开项目导航',exact:true}).click();await p.getByRole('button',{name:'添加项目',exact:true}).click()},p=>p.evaluate(()=>addProjectForm())],
 ['files',async p=>{await p.getByRole('button',{name:'打开项目文件',exact:true}).click();const dialog=p.getByRole('dialog',{name:/项目文件/});await dialog.locator('.project-file-row').filter({hasText:'README.md'}).waitFor();await dialog.locator('.project-markdown h1').waitFor({state:'attached'})},p=>p.evaluate(()=>openProjectFiles())],
 ['trace',p=>p.locator('.composer-foot .session-trace-trigger').click(),p=>p.evaluate(()=>openSessionTrace())],
 ['btw',p=>p.locator('.composer-capabilities').getByRole('button',{name:'BTW',exact:true}).click(),p=>p.evaluate(()=>openBtw())],
 ['skills-picker',p=>p.locator('.skill-picker-trigger').click(),p=>p.evaluate(()=>skillPicker())],
 ['queue',async p=>{await p.locator('.queue-shelf').getByRole('button',{name:'打开消息队列',exact:true}).click();const dialog=p.getByRole('dialog',{name:'消息队列',exact:true});await dialog.getByText('先保留现有快捷键，不要调整键位。',{exact:true}).waitFor();await dialog.locator('.queue-loading').waitFor({state:'hidden'})},p=>p.evaluate(()=>queueModal())],
 ['meta-agent',p=>metaActual(p),p=>metaReference(p)],
 ['meta-editor',p=>metaActual(p,true),p=>metaReference(p,true)],
];
async function measure(page,actual){
 const selectors={header:actual?'.app-header':'.topbar',page:actual?'.page-view:visible, .meta-page':'.page-view:visible',heading:actual?'.page-view h1, .meta-page h1':'.page-view h1',modal:'dialog[open]',modalHeader:'dialog[open] .modal-header',modalBody:'dialog[open] .modal-body',cards:'.settings-cards',btw:actual?'.composer-extras.btw-panel':'.btw-panel',codeEditor:actual?'.meta-code':'.meta-editor-content>textarea'};
 const result={headerText:await page.locator(actual?'.app-header h1':'.topbar h1').allTextContents()};for(const [name,selector]of Object.entries(selectors)){const el=page.locator(selector).first();if(!await el.count()||!await el.isVisible())continue;result[name]=await el.evaluate(e=>{const r=e.getBoundingClientRect(),s=getComputedStyle(e);return{x:r.x,y:r.y,w:r.width,h:r.height,font:s.fontSize,line:s.lineHeight,padding:s.padding,radius:s.borderRadius}})}result.layout=await page.evaluate(()=>{
  const visible=e=>e.getClientRects().length&&getComputedStyle(e).visibility!=='hidden';
  const controls=[...document.querySelectorAll('.resource-card,.provider-group,.settings-card,.skill-picker-row,.session-resource-row,.modal-header,.composer-options,.section-tabs,.settings-navigation,.mobile-manual-form')].filter(visible);
  return {viewportOverflow:document.documentElement.scrollWidth>innerWidth+1,checked:controls.length,overflow:controls.filter(e=>e.scrollWidth>e.clientWidth+2).map(e=>({class:e.className,width:e.clientWidth,content:e.scrollWidth,text:e.textContent.trim().slice(0,60)}))};
 });return result;
}
(async()=>{
 const node=await startFixture(),browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH}),actual=await browser.newPage(),reference=await browser.newPage();actual.setDefaultTimeout(6000);reference.setDefaultTimeout(6000);const results=[];
 node.setQueue([{entryId:'visual-follow-up',queue:'followUp',status:'queued',text:'完成后检查手机端设置抽屉的对比度。'},{entryId:'visual-steer',queue:'steer',status:'queued',text:'先保留现有快捷键，不要调整键位。'}]);
 try{
  for(const page of [actual,reference])await page.addInitScript(()=>localStorage.setItem('rpi-work-appearance-v1',JSON.stringify({theme:'reddit',mode:'light',density:'standard'})));
  const viewports=process.env.REVIEW_ALL_VIEWPORTS?[[1440,1000],[1280,800],[768,1024],[390,844],[320,740]]:[[1440,1000],[390,844]];
  for(const [width,height] of viewports)for(const [name,openActual,openReference]of cases.filter(([name])=>!process.env.REVIEW_PAGE_FILTER||new RegExp(process.env.REVIEW_PAGE_FILTER).test(name))){
   if(width<701&&['providers','provider-connection'].includes(name)){results.push({name,width,notApplicable:true,reason:'Provider configuration is desktop-only in the current client.'});continue;}
   if((name==='devices-advanced'&&width>700)||(name==='devices-phone'&&width>700)||(name==='devices-qr'&&width<701)){results.push({name,width,notApplicable:true,reason:'The current device does not offer this entry: desktop shares its address QR; mobile scans or manually enters a computer address.'});continue;}
   try{
    await actual.setViewportSize({width:1440,height:1000});await actual.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4180/');await connect(actual,node);await actual.setViewportSize({width,height});await actual.evaluate(()=>{document.documentElement.dataset.theme='reddit';document.documentElement.dataset.mode='light'});await reference.setViewportSize({width,height});await reference.goto(process.env.REVIEW_REFERENCE_URL||'http://127.0.0.1:4182/prototype/');await reference.evaluate(()=>{document.documentElement.dataset.theme='reddit';document.documentElement.dataset.mode='light'});await openActual(actual);await openReference(reference);
    await actual.screenshot({path:`artifacts/page-review-${name}-${width}-actual.png`,animations:'disabled'});await reference.screenshot({path:`artifacts/page-review-${name}-${width}-reference.png`,animations:'disabled'});const a=await measure(actual,true),r=await measure(reference,false);results.push({name,width,actual:a,reference:r});console.log('CAPTURED',name,width);
   }catch(error){results.push({name,width,error:error.message});console.log('CAPTURE ERROR',name,width,error.message.split('\n')[0])}
  }
 }finally{fs.writeFileSync(process.env.REVIEW_PAGE_FILTER?'artifacts/ui-pages-review-filtered.json':process.env.REVIEW_ALL_VIEWPORTS?'artifacts/ui-pages-review-all.json':'artifacts/ui-pages-review.json',JSON.stringify({scope:'diagnostic screenshots and structural measurements of current review bundle versus reference; data/availability differences require manual review',capturedAt:new Date().toISOString(),build:JSON.parse(fs.readFileSync('target/parity-web/build.json','utf8').replace(/^\uFEFF/,'')),results},null,2));await browser.close();await node.close()}
})().catch(error=>{console.error(error);process.exitCode=1});

