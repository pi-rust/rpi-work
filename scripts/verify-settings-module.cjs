const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
const formalUrl=process.env.WORK_UI_URL;
(async()=>{
 const fixture=await startFixture(0,{setApiKeySupported:true}),browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'}),page=await browser.newPage({viewport:{width:1200,height:1000}});page.setDefaultTimeout(7000);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 const nav=()=>page.getByRole('navigation',{name:'设置分类',exact:true});
 try{
  await page.addInitScript(()=>{
   window.__tracksStopped=0;window.__cameraMode='decode';
   window.__makeFixtureStream=()=>{const stream=new MediaStream();Object.defineProperty(stream,'getTracks',{value:()=>[{stop(){window.__tracksStopped++}}]});return stream;};
   Object.defineProperty(navigator.mediaDevices,'getUserMedia',{value:()=>window.__cameraMode==='late'?new Promise(resolve=>{window.__cameraPermissionResolve=resolve}):Promise.resolve(window.__makeFixtureStream())});
   HTMLMediaElement.prototype.play=()=>Promise.resolve();HTMLMediaElement.prototype.pause=()=>{};
   window.BarcodeDetector=class{async detect(){return window.__cameraMode==='decode'?[{rawValue:JSON.stringify({v:1,type:'rpi-work-node-address',address:'https://192.168.50.8:3030'})}]:[]}};
  });
  await page.goto(formalUrl||'http://127.0.0.1:4173/target/parity-modules-settings-web/');await page.getByRole('button',{name:formalUrl?'打开设置':'settings',exact:true}).click();
  await nav().getByRole('button',{name:'执行节点',exact:true}).click();if(formalUrl)await page.getByRole('button',{name:'连接其他节点',exact:true}).click();await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);await page.getByRole('button',{name:'连接节点',exact:true}).click();if(formalUrl){await page.getByRole('status').filter({hasText:'在线'}).waitFor();await page.getByRole('button',{name:'返回会话',exact:true}).click();await page.locator('.project-row').filter({hasText:'rpi-work'}).click();await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).waitFor();await page.getByRole('button',{name:'打开设置',exact:true}).click();await nav().getByRole('button',{name:'执行节点',exact:true}).click();}else await page.getByText('节点在线',{exact:true}).first().waitFor();
  pass('computer role retains real execution-node address and connection');
  const qr=page.getByRole('dialog',{name:'手机连接这台电脑',exact:true});
  if(formalUrl){await page.getByRole('button',{name:'手机连接这台电脑',exact:true}).click();await qr.getByLabel('手机可访问的节点地址',{exact:true}).fill('https://192.168.50.8:3030')}
  else{await page.getByLabel('手机可访问的节点地址',{exact:true}).fill('https://192.168.50.8:3030');await page.getByRole('button',{name:'显示地址二维码',exact:true}).first().click()}
  await qr.locator('#device-address-qr-code svg').waitFor();
  await page.addScriptTag({path:path.resolve('prototype/vendor/jsQR.js')});
  const decoded=await qr.locator('#device-address-qr-code svg').evaluate(async svg=>{
   const image=new Image(),url=URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(svg)],{type:'image/svg+xml'}));image.src=url;await image.decode();URL.revokeObjectURL(url);const canvas=document.createElement('canvas');canvas.width=600;canvas.height=600;const context=canvas.getContext('2d');context.fillStyle='#fff';context.fillRect(0,0,600,600);context.drawImage(image,0,0,600,600);const pixels=context.getImageData(0,0,600,600);return window.jsQR(pixels.data,600,600)?.data;
  });
  assert.deepEqual(JSON.parse(decoded),{v:1,type:'rpi-work-node-address',address:'https://192.168.50.8:3030'});assert.doesNotMatch(decoded,/password|apiKey/);await page.keyboard.press('Escape');
  pass('actual QR raster decodes to a versioned address payload without credentials');
  if(formalUrl){await page.setViewportSize({width:390,height:844});await page.locator('.devices-page').getByRole('heading',{name:'连接我的电脑',exact:true}).waitFor()}else await page.getByRole('button',{name:'手机 · 客户端',exact:true}).click();await page.getByRole('button',{name:'打开相机',exact:true}).click();await page.getByText('已读取地址，请核对连接信息后连接。',{exact:true}).waitFor();assert(await page.evaluate(()=>window.__tracksStopped>=1));
  pass('scanner result crosses JavaScript/Rust boundary and closes fake camera tracks');
  await page.evaluate(()=>{window.__cameraMode='late'});await page.getByRole('button',{name:'打开相机',exact:true}).click();await page.waitForFunction(()=>typeof window.__cameraPermissionResolve==='function');await page.getByRole('button',{name:'关闭相机',exact:true}).click();const stopped=await page.evaluate(()=>window.__tracksStopped);await page.evaluate(()=>window.__cameraPermissionResolve(window.__makeFixtureStream()));await page.waitForFunction(before=>window.__tracksStopped>before,stopped);assert(await page.locator('#device-camera').evaluate(v=>!v.srcObject));
  pass('late permission response cannot reactivate a closed camera');
  if(formalUrl)await page.setViewportSize({width:1200,height:1000});await nav().getByRole('button',{name:'Providers',exact:true}).click();
  if(formalUrl){const provider=page.locator('.provider-list > .provider-card, .provider-list > .provider-group').filter({has:page.locator('.provider-card-heading').filter({hasText:'openai'})});await provider.getByRole('button',{name:'用于此会话',exact:true}).click();const modelDialog=page.getByRole('dialog',{name:/选择.*模型/});await modelDialog.getByLabel('选择模型',{exact:true}).selectOption('openai/gpt-6-astra');await modelDialog.getByRole('button',{name:'用于此会话',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.runtime-diagnostics')?.textContent.includes('openai/gpt-6-astra'));if(await modelDialog.isVisible())await page.keyboard.press('Escape');await provider.getByRole('button',{name:/^连接设置(?:：.+)?$/,exact:true}).click()}
  else{await page.getByLabel('选择模型',{exact:true}).selectOption('openai/gpt-6-astra');await page.waitForFunction(()=>document.querySelector('#provider-model')?.value==='openai/gpt-6-astra')}
  assert(fixture.requests.some(r=>r.body.type==='set_model'&&r.body.modelId==='openai/gpt-6-astra'));
  await page.getByLabel('Provider 标识',{exact:true}).fill('openai');await page.getByLabel('API Key',{exact:true}).fill('test-only-review-not-a-key');await page.getByRole('button',{name:/保存.*API Key|提交.*API Key/}).click();await page.getByText(/节点要求重启运行时后生效/).waitFor();assert.equal(await page.getByLabel('API Key',{exact:true}).inputValue(),'');
  pass('provider uses declared real model/key commands and preserves requiresRestart boundary');
  if(formalUrl)await page.keyboard.press('Escape');
  await nav().getByRole('button',{name:'通用',exact:true}).click();const originalTheme=await page.evaluate(()=>document.documentElement.dataset.theme);await page.getByRole('button',formalUrl?{name:'外观与主题',exact:true}:{name:/外观与主题/}).click();const appearance=page.getByRole('dialog',{name:'外观与主题',exact:true});
  await appearance.locator('#appearance-import-file').setInputFiles({name:'appearance.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify({schemaVersion:1,kind:'rpi-work-appearance',preset:'ocean',mode:'dark',density:'compact'}))});await page.waitForFunction(()=>document.documentElement.dataset.theme==='ocean');
  const [download]=await Promise.all([page.waitForEvent('download'),appearance.getByRole('button',{name:'导出偏好 JSON',exact:true}).click()]);assert.deepEqual(JSON.parse(fs.readFileSync(await download.path(),'utf8')),{schemaVersion:1,kind:'rpi-work-appearance',preset:'ocean',mode:'dark',density:'compact'});
  await appearance.locator('#appearance-import-file').setInputFiles({name:'bad.json',mimeType:'application/json',buffer:Buffer.from('{"schemaVersion":999}')});await page.waitForFunction(()=>Boolean(document.querySelector('.theme-error')?.textContent.trim()));assert.equal(await page.evaluate(()=>document.documentElement.dataset.theme),'ocean');
  await page.keyboard.press('Escape');await appearance.waitFor({state:'hidden'});await page.waitForFunction(theme=>document.documentElement.dataset.theme===theme,originalTheme);assert.equal(await page.evaluate(()=>document.documentElement.dataset.theme),originalTheme);
  pass('preference import/export is strict, invalid data is inert, and Escape rolls preview back');
  for(const width of [390,320]){await page.setViewportSize({width,height:844});for(const section of ['执行节点','Providers','通用']){await nav().getByRole('button',{name:formalUrl&&section==='执行节点'?'连接我的电脑':section,exact:true}).click();assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.screenshot({path:`artifacts/ui-settings-module-${section}-${width}.png`});}}
  pass('node, provider and general pages remain bounded at 390/320');assert.deepEqual(errors,[]);pass('no application JavaScript errors');fs.writeFileSync(formalUrl?'artifacts/ui-settings-validation.json':'artifacts/settings-module-validation.json',JSON.stringify({scope:'test-only settings module; camera uses synthetic MediaStream, not real hardware',results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-settings-module-failure.png'});fs.writeFileSync(formalUrl?'artifacts/ui-settings-validation.json':'artifacts/settings-module-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});

