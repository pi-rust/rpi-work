// Two isolated protected nodes verify which address actually receives a password.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
(async()=>{
 const first=await startFixture(0,{authRequired:true}),second=await startFixture(0,{authRequired:true}),browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH}),page=await browser.newPage({viewport:{width:1440,height:1000}}),results=[];
 const pass=name=>{results.push(name);console.log('PASS',name)},posts=node=>node.requests.filter(r=>r.method==='POST'&&r.path==='/api/v1/auth');
 try{
  await page.goto('http://127.0.0.1:4180/');await page.getByRole('button',{name:'打开设置',exact:true}).click();await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
  await page.getByRole('button',{name:'连接其他节点',exact:true}).click();
  const address=page.getByLabel('节点地址',{exact:true}),password=page.getByLabel('节点密码',{exact:true}),connect=page.getByRole('button',{name:'连接节点',exact:true}),login=page.getByRole('button',{name:'登录',exact:true});
  await address.fill(first.url);await connect.click();await page.locator('.server-details [role=status]').filter({hasText:'需要节点密码'}).waitFor();await password.fill('test-only');await address.fill(second.url);await login.click();await page.getByText('请先连接当前地址，再登录',{exact:true}).waitFor({state:'attached'});assert.equal(posts(first).length,0);assert.equal(posts(second).length,0);pass('editing an address without connecting sends no password to either node');
  await connect.click();await page.locator('.server-details [role=status]').filter({hasText:'需要节点密码'}).waitFor();await password.fill('test-only');await login.click();await page.locator('.server-details [role=status]').filter({hasText:'在线'}).waitFor();assert.equal(posts(first).length,0);assert.equal(posts(second).length,1);assert(second.requests.some(r=>r.method==='GET'&&r.path==='/api/v1/auth'&&r.authenticated));pass('sparse login acknowledgement is followed by authoritative cookie authentication on the selected node');
  await connect.click();await page.getByRole('status').filter({hasText:'在线'}).waitFor();assert(second.requests.some(r=>r.path==='/api/v1/projects'&&r.authenticated));pass('reconnecting retains the shared client cookie for protected project requests');
 }catch(error){await page.screenshot({path:'artifacts/ui-auth-scope-failure.png'});throw error}finally{fs.writeFileSync('artifacts/ui-auth-scope-validation.json',JSON.stringify({results},null,2));await browser.close();await first.close();await second.close()}
})().catch(error=>{console.error(error);process.exitCode=1});

