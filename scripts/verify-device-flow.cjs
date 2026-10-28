const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
(async()=>{
 const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
 const results=[],errors=[],payload=JSON.stringify({v:1,type:'rpi-work-node-address',address:'http://192.168.50.8:3030'}),contexts=[];
 const pass=name=>{results.push(name);console.log('PASS',name)};
 const create=async(options={})=>{
  const context=await browser.newContext(options);contexts.push(context);
  await context.addInitScript(value=>{
   window.__scanValue=value;window.__stopped=0;window.__late=false;
   window.__stream=()=>{const stream=new MediaStream();Object.defineProperty(stream,'getTracks',{value:()=>[{stop(){window.__stopped++}}]});return stream;};
   Object.defineProperty(navigator.mediaDevices,'getUserMedia',{value:()=>window.__late?new Promise(resolve=>window.__resolvePermission=resolve):Promise.resolve(window.__stream())});
   HTMLMediaElement.prototype.play=()=>Promise.resolve();HTMLMediaElement.prototype.pause=()=>{};
   window.BarcodeDetector=class{async detect(){return [{rawValue:window.__scanValue}]}};
  },payload);
  await context.route('http://192.168.50.8:3030/**',async route=>{const response=await route.fetch({url:new URL(new URL(route.request().url()).pathname,fixture.url).href});await route.fulfill({response})});
  await context.route('https://api.github.com/repos/pi-rust/rpi-work/releases/latest',route=>route.fulfill({status:404,body:'{}',headers:{'access-control-allow-origin':'*'}}));
  const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(8000);
  await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4173/target/parity-web/');return page;
 };
 const open=async(page,title)=>{await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:title,exact:true}).click()};
 try{
  const desktop=await create({viewport:{width:1280,height:900}});await open(desktop,'执行节点');
  assert.equal(await desktop.getByLabel('节点地址',{exact:true}).count(),0);assert.equal(await desktop.locator('#device-camera').count(),0);assert.equal(await desktop.locator('.connection-role-switch').count(),0);await desktop.getByRole('button',{name:'连接本机节点',exact:true}).waitFor();pass('desktop starts with its local node and has no phone role or scanner');
  await desktop.getByRole('button',{name:'连接其他节点',exact:true}).click();await desktop.getByLabel('节点地址',{exact:true}).fill(fixture.url);await desktop.getByRole('button',{name:'连接节点',exact:true}).click();await desktop.getByRole('status').filter({hasText:'在线'}).first().waitFor();assert(fixture.requests.some(request=>request.path==='/api/v1/health'));pass('desktop still connects another real fixture node');
  await desktop.getByRole('button',{name:'手机连接这台电脑',exact:true}).click();const dialog=desktop.getByRole('dialog',{name:'手机连接这台电脑',exact:true});await dialog.getByLabel('手机可访问的节点地址',{exact:true}).fill('http://192.168.50.8:3030');await dialog.locator('#device-address-qr-code svg').waitFor();const qrImage=await dialog.locator('#device-address-qr-code svg').screenshot({animations:'disabled'});await desktop.keyboard.press('Escape');await desktop.screenshot({path:'artifacts/node-flow-desktop.png',animations:'disabled'});pass('desktop exposes the computer address QR for mobile clients');
  const phone=await create({viewport:{width:390,height:844},isMobile:true,hasTouch:true,userAgent:'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/130.0.0.0 Mobile Safari/537.36'});
  await phone.getByRole('button',{name:'连接我的电脑',exact:true}).first().click();await phone.locator('.devices-page').getByRole('heading',{name:'连接我的电脑',exact:true}).waitFor();assert.equal(await phone.locator('.connection-role-switch,.mobile-connect-tabs,.pair-link-entry').count(),0);assert.equal(await phone.getByLabel('节点地址',{exact:true}).count(),0);assert.equal(await phone.getByRole('button',{name:'连接其他节点',exact:true}).count(),0);assert(!/粘贴|高级|服务端/.test(await phone.locator('.devices-page').innerText()));assert.equal(await phone.getByRole('button',{name:'连接这台电脑',exact:true}).count(),0);await phone.screenshot({path:'artifacts/node-flow-phone.png',animations:'disabled'});pass('mobile entry is connect my computer and only offers QR scanning');
  await phone.getByRole('button',{name:'打开相机',exact:true}).click();await phone.getByText('已识别电脑',{exact:true}).waitFor();assert(await phone.evaluate(()=>window.__stopped>=1));await phone.getByRole('button',{name:'连接这台电脑',exact:true}).click();await phone.getByRole('status').filter({hasText:'在线'}).first().waitFor();pass('scanned address requires confirmation and connects through the node API');
  await phone.evaluate(()=>{window.BarcodeDetector=undefined});await phone.locator('#device-qr-image').setInputFiles({name:'computer.png',mimeType:'image/png',buffer:qrImage});await phone.getByText('已读取图片地址，请核对后连接。',{exact:true}).waitFor();await phone.locator('.scanned-computer code').filter({hasText:'http://192.168.50.8:3030'}).waitFor();pass('album scanning decodes a real generated QR without BarcodeDetector');
  await phone.evaluate(()=>{window.__late=true});await phone.getByRole('button',{name:'打开相机',exact:true}).click();await phone.waitForFunction(()=>typeof window.__resolvePermission==='function');await phone.getByRole('button',{name:'关闭相机',exact:true}).click();const stopped=await phone.evaluate(()=>window.__stopped);await phone.evaluate(()=>window.__resolvePermission(window.__stream()));await phone.waitForFunction(value=>window.__stopped>value,stopped);assert(await phone.locator('#device-camera').evaluate(video=>!video.srcObject));pass('closing the scanner also stops a late permission response');
  for(const width of [320,1024]){await phone.setViewportSize({width,height:844});await phone.locator('.devices-page').getByRole('heading',{name:'连接我的电脑',exact:true}).waitFor();assert(await phone.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));}pass('phone mode survives narrow and landscape tablet widths without overflow');
  await phone.reload();await open(phone,'连接我的电脑');await phone.locator('.phone-current-node [role=status]').filter({hasText:'在线'}).waitFor();assert.equal(await phone.getByLabel('节点地址',{exact:true}).count(),0);pass('mobile automatically reconnects its previously confirmed computer after reopening');
  await desktop.setViewportSize({width:390,height:844});await desktop.locator('.devices-page').getByRole('heading',{name:'连接我的电脑',exact:true}).waitFor();await desktop.setViewportSize({width:1280,height:900});await desktop.locator('.devices-page').getByRole('heading',{name:'电脑运行，手机接续',exact:true}).waitFor();assert.equal(await desktop.locator('#device-camera').count(),0);pass('browser responsive mode changes preserve distinct desktop and mobile flows');
  assert.deepEqual(errors,[]);pass('no browser application errors');
 }finally{fs.writeFileSync('artifacts/node-flow-validation.json',JSON.stringify({scope:'Formal Dioxus WASM; isolated node fixture and synthetic camera permissions; album QR is actually decoded',results,errors},null,2));await browser.close();await fixture.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
