// Production UI + isolated fixture. Gates are causal assertions, not latency benchmarks.
const assert=require('node:assert/strict'),fs=require('node:fs');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
const WEB=process.env.WORK_UI_URL||'http://127.0.0.1:14180/';
const dense=prefix=>Array.from({length:20},(_,i)=>[
 {entryId:`${prefix}-user-${i}`,role:'user',content:`${prefix} 用户问题 ${i}`},
 ...Array.from({length:20},(_,j)=>({entryId:`${prefix}-assistant-${i}-${j}`,role:'assistant',content:`fixture ${i}/${j}`}))
]).flat();
(async()=>{
 const fixture=await startFixture(0,{pagedHistory:true,nativeQueries:true});
 fixture.setMessages(dense('dark'));fixture.setMessages(dense('mobile'),'mobile');
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
 const context=await browser.newContext({viewport:{width:1440,height:1000}});
 const errors=[],blocked=[],held=[],results=[];let gating=false,released=false,closing=false,release;
 const gate=new Promise(resolve=>{release=()=>{released=true;resolve();};});
 const claimed=new Set();
 const pass=name=>{results.push(name);console.log('PASS',name);};
 await context.route('**/*',async route=>{
  const request=route.request(),url=new URL(request.url());
  if(url.hostname==='api.github.com')return route.fulfill({status:404,body:'{}'});
  if(![new URL(WEB).origin,new URL(fixture.url).origin].includes(url.origin)){blocked.push(url.origin+url.pathname);return route.abort();}
  if(!url.pathname.startsWith('/api/'))return route.continue();
  if(url.pathname.endsWith('/events')&&url.origin===new URL(fixture.url).origin)return route.continue();
  const body=request.method()==='POST'?(request.postDataJSON()||{}):{};
  if(!['GET','OPTIONS'].includes(request.method())&&!url.pathname.endsWith('/open')&&!(url.pathname.endsWith('/commands')&&body.type?.startsWith('get_'))){blocked.push(request.method()+' '+url.pathname);return route.abort();}
  const key=url.pathname.endsWith('/files')?'files':url.pathname.endsWith('/git/status')?'git':url.pathname.includes('/sessions/dark/')&&['get_queue','get_state'].includes(body.type)?body.type:null;
  const hold=gating&&key&&!claimed.has(key);
  if(hold){claimed.add(key);held.push(key);await gate;}
  if(closing)return route.abort().catch(()=>{});
  const response=await route.fetch({url:new URL(url.pathname+url.search,fixture.url).href});
  if(url.pathname.endsWith('/snapshot')){
   const json=await response.json();json.events=[];delete json.latestSettlement;
   return route.fulfill({response,json});
  }
  if(hold&&key==='files'){
   const json=await response.json();json.entries=[{name:'STALE-DARK-FILE',path:'STALE-DARK-FILE',kind:'file'}];
   return route.fulfill({response,json});
  }
  return route.fulfill({response});
 });
 const page=await context.newPage();page.setDefaultTimeout(12000);page.on('pageerror',e=>errors.push(e.message));
 const requests=(id,action)=>fixture.requests.filter(r=>r.path===`/api/v1/sessions/${id}/${action}`);
 async function select(id){
  const row=page.locator(`[data-session-id="${id}"] .session-item`);
  if(!await row.isVisible())await page.locator('[data-project-id="rpi-work"] .project-toggle').click();
  await row.click();
 }
 try{
  await page.goto(WEB);
  await page.getByRole('button',{name:'打开设置',exact:true}).click();
  await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
  await page.getByRole('button',{name:'连接其他节点',exact:true}).click();
  await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
  await page.getByRole('button',{name:'连接节点',exact:true}).click();
  await page.getByRole('status').filter({hasText:'在线'}).first().waitFor();
  await page.getByRole('button',{name:'返回会话',exact:true}).click();
  // The fixture may auto-open its most recent session on connection. Exclude
  // that setup open from the later explicit session-switch request counts.
  await page.waitForTimeout(300);
  const mobileOpens=requests('mobile','open').length;
  const mobileSnapshots=requests('mobile','snapshot').length;
  gating=true;await select('dark');
  await page.locator('.transcript [data-history-id="dark-user-19"]').waitFor({timeout:5000});
  await page.locator('.conversation-timeline[data-native-timeline="true"] .timeline-node').first().waitFor({timeout:5000});
  for(let i=0;i<100&&held.length<4;i++)await page.waitForTimeout(20);
  assert.deepEqual([...held].sort(),['files','get_queue','get_state','git']);
  assert.equal(released,false);pass('messages and native timeline render while all four supplemental reads remain blocked');
  // RuntimeDialogs deliberately keeps a separate fresh observation for its
  // compaction/write guard. The open/connection lane shares the other one.
  assert.equal(requests('dark','access').length,2);assert.equal(requests('dark','open').length,1);
  assert.equal(requests('dark','snapshot').length,1);
  assert.equal(new URLSearchParams(requests('dark','snapshot')[0].query).get('limit'),'48');
  assert(requests('dark','message-summaries').length>=1);
  assert.equal(fixture.requests.filter(r=>r.body.type==='get_messages').length,0);
  assert(await page.locator('.transcript [data-history-id]').count()<=48);
  pass('connection reuses access (dialog guard has its own); one open/bounded snapshot and no legacy scan');
  fixture.emit({type:'message_end',message:{entryId:'dark-live',role:'assistant',content:'事件消费不等待补载'}},'dark');
  await page.getByText('事件消费不等待补载',{exact:true}).waitFor({timeout:5000});
  assert.equal(released,false);pass('live event is consumed before optional metadata is released');
  await select('mobile');
  await page.locator('.transcript [data-history-id="mobile-user-19"]').waitFor();
  release();await page.waitForTimeout(800);
  assert.equal(await page.locator('.transcript [data-history-id^="dark-"]').count(),0);
  assert.equal(await page.getByText('STALE-DARK-FILE',{exact:true}).count(),0);
  assert(requests('mobile','access').length>=1);
  assert.equal(requests('mobile','open').length-mobileOpens,1);
  assert.equal(requests('mobile','snapshot').length-mobileSnapshots,1);
  pass('switching session drops delayed old-scope results');
  assert.deepEqual(errors,[]);assert.deepEqual(blocked,[]);
  fs.writeFileSync('artifacts/session-loading-validation.json',JSON.stringify({passed:true,results,held,errors,blocked,requests:fixture.requests,scope:'isolated causal gates; not a production or Android device benchmark'},null,2));
 }catch(e){
  fs.writeFileSync('artifacts/session-loading-failure.json',JSON.stringify({held,requests:fixture.requests,errors,blocked},null,2));
  throw e;
 }finally{closing=true;release();await page.waitForTimeout(100);await context.close();await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
