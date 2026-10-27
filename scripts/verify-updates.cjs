const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH||'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
 const page=await browser.newPage({viewport:{width:1280,height:900}}),results=[],errors=[];
 let mode='empty',requests=0,delayedRoute,delayedReady;
 const delayedRequest=new Promise(resolve=>{delayedReady=resolve});
 page.on('pageerror',e=>errors.push(e.message));
 await page.route('https://api.github.com/repos/pi-rust/rpi-work/releases/latest',async route=>{
  requests++;
  if(mode==='delayed'){delayedRoute=route;delayedReady();return;}
  await route.fulfill({status:mode==='error'?503:mode==='empty'?404:200,contentType:'application/json',headers:{'access-control-allow-origin':'*'},body:JSON.stringify(mode==='new'?{tag_name:'v0.2.0',body:'更快的会话更新\n<script>not-executable</script>'}: {message:'fixture'})});
 });
 const pass=name=>{results.push(name);console.log('PASS',name)};
 const open=async()=>{await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类',exact:true}).getByRole('button',{name:'应用更新',exact:true}).click()};
 try{
  await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4173/target/parity-web/');
  await open();await page.getByRole('status').filter({hasText:'当前已是最新已发布版本'}).waitFor();pass('no published release is a nonblocking current state');
  const automatic=page.getByRole('checkbox',{name:'自动检查更新'});await automatic.uncheck();
  await page.reload();await open();await automatic.waitFor();assert.equal(await automatic.isChecked(),false);await page.getByText('尚未检查更新',{exact:true}).waitFor();pass('automatic-check preference persists across reload');
  mode='error';await page.getByRole('button',{name:'检查更新',exact:true}).click();await page.getByRole('status').filter({hasText:'检查更新失败'}).waitFor();assert(await page.getByRole('button',{name:'检查更新',exact:true}).isEnabled());pass('network failure permits manual retry');
  mode='delayed';await page.getByRole('button',{name:'检查更新',exact:true}).click();await Promise.race([delayedRequest,new Promise((_,reject)=>setTimeout(()=>reject(new Error('Missing delayed check')),7000))]);await page.getByRole('navigation',{name:'设置分类',exact:true}).getByRole('button',{name:'通用',exact:true}).click();await delayedRoute.fulfill({status:200,contentType:'application/json',headers:{'access-control-allow-origin':'*'},body:JSON.stringify({tag_name:'v0.2.0',body:'更快的会话更新\n<script>not-executable</script>'})});await page.getByRole('complementary',{name:'新版本提醒'}).getByRole('button',{name:'查看更新'}).click();await page.getByRole('status').filter({hasText:'发现新版本 0.2.0'}).waitFor();pass('checking survives leaving the update page and the reminder reopens its completed result');assert.equal(await page.getByRole('button',{name:'下载更新',exact:true}).count(),0);await page.getByRole('button',{name:'重新加载',exact:true}).waitFor();assert.equal(await page.locator('.update-notes script').count(),0);pass('web shows deployment/reload flow and renders release notes as inert text');
  await page.getByRole('complementary',{name:'新版本提醒'}).getByRole('button',{name:'稍后提醒'}).click();assert.equal(await page.getByRole('complementary',{name:'新版本提醒'}).count(),0);pass('version reminder can be dismissed');
  for(const width of [1280,390,320]){
   await page.setViewportSize({width,height:900});await page.screenshot({path:`artifacts/update-settings-${width}.png`,animations:'disabled'});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));assert(await page.locator('.update-card').evaluate(element=>{const rect=element.getBoundingClientRect();return rect.left>=0&&rect.right<=innerWidth}));if(width<=700)await page.waitForFunction(()=>document.querySelector('.sidebar').getBoundingClientRect().right<=0);
  }
  pass('update settings fit desktop, 390px and 320px layouts');assert.deepEqual(errors,[]);pass('no application JavaScript errors');
 }finally{
  fs.writeFileSync('artifacts/update-ui-validation.json',JSON.stringify({scope:'formal WASM client; GitHub responses are routed fixtures, no release publication or real installation',results,errors,requests},null,2));await browser.close();
 }
})().catch(error=>{console.error(error);process.exitCode=1});
