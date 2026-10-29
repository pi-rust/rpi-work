const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require('../support/browser.cjs');
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH}),page=await browser.newPage({viewport:{width:1150,height:950}});page.setDefaultTimeout(6000);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 async function answer(id){await page.waitForFunction(id=>{try{return JSON.parse(document.querySelector('.module-answers')?.textContent).requestId===id}catch{return false}},id);return JSON.parse(await page.locator('.module-answers').innerText());}
 try{
  await page.goto('http://127.0.0.1:4182/modules/conversation/');await page.getByRole('button',{name:'conversation',exact:true}).click();
  await page.getByRole('heading',{name:'公开回答',exact:true}).waitFor();assert.match(await page.locator('pre code').first().innerText(),/fn main/);assert.equal(await page.locator('a[href^="javascript:"]').count(),0);assert.equal(await page.evaluate(()=>window.__markdownExecuted),undefined);assert.match(await page.locator('.conversation-module-content').innerText(),/<script>/);
  pass('assistant Markdown renders structure while script markup and unsafe links remain inert');
  const submit=id=>page.getByRole('button',{name:`${id} · 提交回答`,exact:true});
  assert(await submit('review-single').isDisabled());await page.getByRole('radio',{name:'review-single · Rust',exact:true}).check();await submit('review-single').click();assert.deepEqual(await answer('review-single'),{requestId:'review-single',answer:{values:['Rust'],kind:'selector'}});
  pass('string selector options produce native answer envelope without prompt');
  await page.getByRole('button',{name:'Simulate response failure',exact:true}).click();await page.getByRole('alert').filter({hasText:/节点拒绝/}).waitFor();assert(await page.getByRole('radio',{name:'review-single · Rust',exact:true}).isChecked());await page.waitForFunction(()=>[...document.querySelectorAll('button')].some(b=>b.getAttribute('aria-label')==='review-single · 提交回答'&&!b.disabled));
  pass('failed reply retains selected answer and releases duplicate-submit guard');
  await page.getByRole('button',{name:'multi',exact:true}).click();await page.getByRole('checkbox',{name:'review-multi · 桌面',exact:true}).check();await page.getByRole('checkbox',{name:'review-multi · 手机',exact:true}).check();await submit('review-multi').click();assert.deepEqual(await answer('review-multi'),{requestId:'review-multi',answer:{values:['桌面','手机'],kind:'selector'}});
  pass('multi-select keeps request id and returns ordered values');
  await page.getByRole('button',{name:'free',exact:true}).click();await page.getByLabel('review-free · 自由回答',{exact:true}).fill('中文理由');await submit('review-free').click();assert.deepEqual(await answer('review-free'),{requestId:'review-free',answer:{values:['中文理由'],value:'中文理由',text:'中文理由',kind:'input'}});
  pass('free text returns native input envelope');
  await page.getByRole('button',{name:'permission',exact:true}).click();assert.equal(await page.locator('.ask-card textarea').count(),0);await page.getByRole('radio',{name:'review-permission · 允许一次',exact:true}).check();await submit('review-permission').click();assert.deepEqual(await answer('review-permission'),{requestId:'review-permission',answer:{values:['允许一次'],kind:'selector'}});
  pass('permission requires explicit semantic option and has no free-text shortcut');
  await page.getByRole('button',{name:'single',exact:true}).click();await page.getByRole('button',{name:'review-single · 取消回答',exact:true}).click();assert.deepEqual(await answer('review-single'),{requestId:'review-single',answer:null});
  pass('cancel emits only matching request id with no answer');
  await page.locator('details').filter({hasText:'read'}).first().locator('summary').click();assert.match(await page.locator('details').filter({hasText:'read'}).first().innerText(),/public tool result/);
  for(const width of [390,320]){await page.setViewportSize({width,height:844});await page.screenshot({path:`artifacts/ui-conversation-module-${width}.png`});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));}
  pass('inline tools and ask cards remain bounded at 390/320');assert.deepEqual(errors,[]);pass('no application JavaScript errors');
  fs.writeFileSync('artifacts/conversation-module-validation.json',JSON.stringify({scope:'test-only module harness; parent transport integration required',results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-conversation-module-failure.png'});fs.writeFileSync('artifacts/conversation-module-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
