// Read-only integration check. Never creates/opens a session or invokes a model.
const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const base=process.env.RPI_WORK_NODE_URL||'http://127.0.0.1:43340';
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'}),page=await browser.newPage({viewport:{width:1440,height:1000}});page.setDefaultTimeout(7000);
 const observed=[],errors=[];page.on('pageerror',error=>errors.push(error.message));
 page.on('response',response=>{const url=new URL(response.url());if(url.origin===base&&url.pathname.startsWith('/api/'))observed.push({method:response.request().method(),path:url.pathname,status:response.status()});});
 try{
  // The real node deliberately rejects foreign Origin headers. Route only local
  // static review assets in this isolated browser, leaving all /api calls real.
  await page.route(`${base}/__work-review/**`,async route=>{
   const name=decodeURIComponent(new URL(route.request().url()).pathname.slice('/__work-review/'.length))||'index.html';
   const root=path.resolve('target/parity-web'),target=path.resolve(root,name);
   if(!target.startsWith(root+path.sep)||!['.html','.js','.wasm'].includes(path.extname(target)))return route.abort();
   await route.fulfill({path:target,contentType:name.endsWith('.wasm')?'application/wasm':name.endsWith('.js')?'text/javascript':'text/html'});
  });
  await page.goto(base+'/__work-review/index.html');await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
  await page.getByLabel('节点地址',{exact:true}).fill(base+'/');await page.getByRole('button',{name:'连接节点',exact:true}).click();await page.getByRole('status').filter({hasText:'在线'}).waitFor();
  for(const endpoint of ['health','auth','capabilities','sessions','projects'])assert(observed.some(r=>r.path===`/api/v1/${endpoint}`&&r.status===200),`missing successful ${endpoint}`);
  assert(observed.every(r=>r.method==='GET'),'readonly check made a mutation');assert.deepEqual(errors,[]);
  const report={scope:'actual isolated rpi-http; same-origin review assets fulfilled locally in isolated browser; all API requests real and readonly; no session opened and no model invoked',base,checkedAt:new Date().toISOString(),passed:true,observed,errors};fs.writeFileSync('artifacts/real-node-readonly-validation.json',JSON.stringify(report,null,2));console.log('PASS actual rpi-http same-origin connection, auth state, capabilities and project/session catalogs; GET only');
 }catch(error){const status=await page.locator('.device-status-panel p.meta').allTextContents();fs.writeFileSync('artifacts/real-node-readonly-validation.json',JSON.stringify({passed:false,observed,errors,status,error:error.message},null,2));console.error(JSON.stringify({observed,status}));throw error;}finally{await browser.close();}
})().catch(error=>{console.error(error.message);process.exitCode=1});
