// Formal standalone WASM: failed attempts remain visible before a node is authorized.
const assert=require('node:assert/strict'),fs=require('node:fs');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
(async()=>{
 const node=await startFixture(0,{authRequired:true});
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
 const page=await browser.newPage({viewport:{width:390,height:844}});
 const address='http://192.168.50.9:3030',results=[],errors=[];
 let mode='unavailable',passed=false;
 page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(8000);
 await page.addInitScript(()=>{
  const timer=window.setTimeout.bind(window);
  window.setTimeout=(fn,delay,...args)=>timer(fn,delay===15000?250:delay,...args);
 });
 await page.route('https://api.github.com/**',r=>r.abort());
 await page.route(`${address}/**`,async route=>{
  if(mode==='timeout') await new Promise(resolve=>setTimeout(resolve,650));
  if(mode!=='normal') { await route.fulfill({status:503,headers:{'access-control-allow-origin':'*'},contentType:'application/json',body:'{"error":"fixture unavailable"}'}).catch(()=>{});return; }
  const response=await route.fetch({url:new URL(new URL(route.request().url()).pathname,node.url).href});
  await route.fulfill({response});
 });
 try{
  await page.goto(process.env.WORK_UI_URL);
  await page.getByRole('button',{name:'连接我的电脑',exact:true}).first().click();
  const input=page.getByLabel('电脑节点地址',{exact:true}),password=page.getByLabel('电脑节点密码',{exact:true});
  const submit=page.getByRole('button',{name:'连接电脑',exact:true});
  const status=pattern=>page.locator('.connection-panel [role=status]').filter({hasText:pattern}).waitFor();
  await input.fill(address);await submit.click();await status(/连接失败/);
  assert.equal(await page.locator('.phone-current-node').count(),0);
  results.push('unreachable attempt exposes its failure before any node is authorized');
  mode='timeout';await submit.click();await status(/超时/);
  results.push('connection deadline remains visible with an actionable retry');
  await page.waitForTimeout(700);
  mode='normal';await password.fill('wrong-fixture-password');await submit.click();await status(/登录失败/);
  assert.equal(await password.inputValue(),'');
  results.push('wrong password exposes the authoritative failure and clears the secret');
  await password.fill('test-only');await submit.click();
  await page.locator('.phone-current-node [role=status]').filter({hasText:'在线'}).waitFor();
  assert.equal(await page.locator('input[type=password]').count(),0);assert.deepEqual(errors,[]);
  assert(!node.requests.some(r=>r.body?.type==='prompt'||r.body?.command==='prompt'));
  results.push('retry after all failures authenticates without stale errors or prompt calls');
  passed=true;results.forEach(x=>console.log('PASS',x));
 }finally{
  fs.writeFileSync('artifacts/unified-ui-connection-failures.json',JSON.stringify({passed,scope:'Formal WASM + isolated HTTP fixtures; deadline clock accelerated; not native device',results,errors},null,2));
  await browser.close();await node.close();
 }
})().catch(e=>{console.error(e);process.exitCode=1;});
