const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
async function until(predicate){for(let i=0;i<100;i++){if(predicate())return;await new Promise(resolve=>setTimeout(resolve,50));}throw new Error('Fixture request did not arrive');}
(async()=>{
 const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
 const page=await browser.newPage({viewport:{width:1200,height:950}});page.setDefaultTimeout(6500);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 try{
  await page.goto('http://127.0.0.1:4173/target/parity-modules-extras-web/');await page.getByRole('button',{name:'extras',exact:true}).click();
  await page.getByLabel('Fixture node URL').fill(fixture.url);await page.getByRole('button',{name:'Connect fixture',exact:true}).click();
  const input=page.getByLabel('选择本地附件，可多选');
  await input.setInputFiles([{name:'review.md',mimeType:'text/markdown',buffer:Buffer.from('# 本地附件\n只由用户显式引用。')},{name:'image.png',mimeType:'image/png',buffer:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aL1cAAAAASUVORK5CYII=','base64')},{name:'untrusted.html',mimeType:'text/html',buffer:Buffer.from('<script>window.__attachmentExecuted=true</script>')}]);
  await page.getByRole('button',{name:'预览 review.md',exact:true}).waitFor();assert.equal(await page.locator('.draft-chip').count(),3);
  await page.getByRole('button',{name:'预览 review.md',exact:true}).click();await page.getByRole('button',{name:'引用文本到草稿',exact:true}).click();assert.match(await page.locator('.module-quote').innerText(),/# 本地附件/);assert.equal(fixture.requests.filter(r=>r.body.type==='prompt').length,0);
  pass('local text preview explicitly quotes into draft without sending prompt');
  await page.getByRole('button',{name:'预览 untrusted.html',exact:true}).click();assert.equal(await page.evaluate(()=>window.__attachmentExecuted),undefined);await page.keyboard.press('Escape');
  await page.getByRole('button',{name:'预览 image.png',exact:true}).click();assert(await page.getByRole('dialog').getByRole('img',{name:'image.png 的本地预览',exact:true}).getAttribute('src').then(src=>src.startsWith('data:image/png;base64,')));await page.keyboard.press('Escape');
  pass('raster preview works and HTML attachments remain inert text');
  await page.getByRole('button',{name:'Session B',exact:true}).click();assert.equal(await page.locator('.draft-chip').count(),0);await page.getByRole('button',{name:'Session A',exact:true}).click();assert.equal(await page.locator('.draft-chip').count(),3);
  await input.setInputFiles({name:'too-large.bin',mimeType:'application/octet-stream',buffer:Buffer.alloc(5*1024*1024+1)});await page.getByRole('alert').filter({hasText:'超出附件限制'}).waitFor();assert.equal(await page.locator('.draft-chip').count(),3);
  await page.getByRole('button',{name:'移除 untrusted.html',exact:true}).click();assert.equal(await page.locator('.draft-chip').count(),2);
  pass('attachment scope isolation, oversize rejection and removal preserve other files');
  await page.getByRole('button',{name:'Open BTW',exact:true}).click();const btw=page.getByRole('complementary',{name:'BTW 独立旁问会话',exact:true});
  await btw.getByLabel('旁问消息').fill('这是独立旁问');await btw.getByRole('button',{name:'发送旁问',exact:true}).click();
  await until(()=>fixture.requests.some(r=>r.path.endsWith('/events')));
  const created=fixture.requests.find(r=>r.method==='POST'&&r.path==='/api/v1/sessions');assert.equal(created.body.projectId,'rpi-work');
  const sent=fixture.requests.find(r=>r.body.type==='prompt');assert(sent);assert.equal(sent.body.message,'这是独立旁问');assert(!sent.path.includes('/dark/'));
  const id=sent.path.split('/')[4];
  fixture.emit({type:'agent_start',runId:'btw-real-fixture'},id);fixture.emit({type:'message_update',message:{role:'assistant',entryId:'btw-answer'},eventType:'text_delta',delta:'这是公开旁问回答。'},id);
  await btw.getByText('这是公开旁问回答。',{exact:true}).waitFor();
  pass('BTW creates independent real HTTP session and streams public text');
  await btw.getByLabel('旁问消息').fill('完成后继续解释');await btw.getByRole('button',{name:'排队后续旁问',exact:true}).click();
  await until(()=>fixture.requests.some(r=>r.body.type==='queue_message'));
  assert(fixture.requests.some(r=>r.body.type==='queue_message'&&r.body.deliverAs==='followUp'&&r.path.includes(`/${id}/`)));
  fixture.emit({type:'agent_end'},id);fixture.emit({type:'agent_settled',runId:'btw-real-fixture',outcome:'completed'},id);
  await btw.getByRole('button',{name:'引用到主输入框',exact:true}).click();assert.match(await page.locator('.module-quote').innerText(),/这是公开旁问回答/);
  pass('busy BTW queues followUp and quotes only explicit public answer');
  await btw.getByLabel('旁问消息').fill('关闭后保留此草稿');await btw.getByRole('button',{name:'关闭旁问面板',exact:true}).click();await page.getByRole('button',{name:'Open BTW',exact:true}).click();
  assert.equal(await btw.getByLabel('旁问消息').inputValue(),'关闭后保留此草稿');await btw.getByText(`sessionId: ${id}`,{exact:true}).waitFor();await btw.getByText('这是公开旁问回答。',{exact:true}).waitFor();
  assert.equal(fixture.requests.filter(r=>r.method==='POST'&&r.path==='/api/v1/sessions').length,1);
  pass('closing and reopening retains BTW session, history and draft');
  await page.getByRole('button',{name:'Session B',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('.btw-composer textarea')?.value==='');
  fixture.controls.failPrompt=true;await btw.getByLabel('旁问消息').fill('重试时不要丢失');await btw.getByRole('button',{name:'发送旁问',exact:true}).click();
  await btw.getByRole('alert').filter({hasText:/拒绝|未确认/}).waitFor();assert.equal(await btw.getByLabel('旁问消息').inputValue(),'重试时不要丢失');
  fixture.controls.failPrompt=false;await btw.getByRole('button',{name:'发送旁问',exact:true}).waitFor();await btw.getByRole('button',{name:'发送旁问',exact:true}).click();
  await until(()=>fixture.requests.filter(r=>r.body.type==='prompt'&&r.body.message==='重试时不要丢失').length===2);
  assert.equal(fixture.requests.filter(r=>r.body.type==='queue_message'&&r.body.message==='重试时不要丢失').length,0);
  pass('rejected BTW send retains draft and retries prompt after state sync');
  for(const width of [390,320]){await page.setViewportSize({width,height:844});await page.screenshot({path:`artifacts/ui-extras-module-${width}.png`});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));}
  pass('390/320 attachments and BTW remain bounded');assert.deepEqual(errors,[]);pass('no application JavaScript errors');
  fs.writeFileSync('artifacts/extras-module-validation.json',JSON.stringify({scope:'test-only module harness',results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-extras-module-failure.png'});fs.writeFileSync('artifacts/extras-module-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
