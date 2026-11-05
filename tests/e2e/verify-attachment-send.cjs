// Production WASM + synthetic HTTP/SSE only; no Agent or provider is started.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const PROFILE = {contentBlocksVersion:1,image:true,imageMimeTypes:['image/png','image/jpeg'],maxImages:4,maxImageBytes:262144,maxTotalImageBytes:524288,maxImageBase64Bytes:349528,maxPromptBytes:786432,maxTextBytes:32768,maxContentBlocks:64,maxImageSide:2000,maxImagePixels:4000000,imageDecodeAllocBudgetBytes:33554432,frameBytes:1048576};
(async () => {
  const fixture = await startFixture(0, { promptContent: PROFILE });
  const browser = await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  page.setDefaultTimeout(12000);
  const results=[], errors=[], geometry=[];
  const pass = name => { results.push(name); console.log('PASS',name); };
  const field=()=>page.locator('.composer-main textarea');
  const input=()=>page.locator('.attachment-picker input[type=file]');
  const chips=()=>page.locator('.draft-attachments .draft-chip');
  const prompts=()=>fixture.requests.filter(r=>r.body?.type==='prompt');
  const choose=async files=>{await input().setInputFiles(files);await page.waitForFunction(()=>!document.querySelector('.attachment-files-slot')?.textContent.includes('正在读取本地附件'));assert.equal(await chips().count(),files.length);};
  const send=()=>page.getByRole('button',{name:'发送消息',exact:true});
  const ready=async()=>page.waitForFunction(()=>{const b=document.querySelector('.send-button');return b&&!b.disabled&&b.getAttribute('aria-label')==='发送消息';});
  const settle=async(body,index)=>{
    fixture.emit({type:'agent_start',runId:`attachment-run-${index}`});
    fixture.emit({type:'message_end',message:{entryId:`attachment-user-${index}`,role:'user',content:body.contentBlocks||body.message}});
    fixture.emit({type:'message_end',message:{entryId:`attachment-answer-${index}`,role:'assistant',content:[{type:'text',text:`fixture accepted ${index}`}]}});
    fixture.emit({type:'agent_settled',runId:`attachment-run-${index}`,outcome:'completed',pendingResume:false});
    await page.locator(`[data-history-id="attachment-user-${index}"]`).waitFor();
  };
  page.on('pageerror',e=>errors.push(e.message));
  try {
    await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:14180/');
    const samples=await page.evaluate(()=>{
      const c=document.createElement('canvas');c.width=640;c.height=360;let x=c.getContext('2d');x.fillStyle='#397ecb';x.fillRect(0,0,640,360);x.fillStyle='white';x.font='28px sans-serif';x.fillText('Actual attachment bytes',25,90);
      const small=c.toDataURL('image/png').split(',')[1];
      c.width=2800;c.height=2100;x=c.getContext('2d');x.fillStyle='#327657';x.fillRect(0,0,c.width,c.height);x.fillStyle='#fff';x.fillText('Large photo',40,70);
      return {small,large:c.toDataURL('image/jpeg',.95).split(',')[1]};
    });
    const photo={name:'截图😀.png',mimeType:'image/png',buffer:Buffer.from(samples.small,'base64')};
    const text={name:'说明.md',mimeType:'text/markdown',buffer:Buffer.from('# 完整附件\n安全的文本内容 😀\n最后一行。')};
    await page.getByRole('button',{name:'打开设置',exact:true}).click();
    await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await page.getByRole('button',{name:'连接其他节点',exact:true}).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await page.getByRole('button',{name:'连接节点',exact:true}).click();
    await page.locator('.server-details [role=status]').filter({hasText:'在线'}).waitFor();
    await page.getByRole('button',{name:'返回会话',exact:true}).click();
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.waitForFunction(()=>document.querySelector('.composer-main textarea')?.dataset.imeReady==='true');
    await field().fill('请检查图片和说明');
    await choose([photo,text]);
    for(const name of [photo.name,text.name]) {
      await page.getByRole('button',{name:`预览 ${name}`,exact:true}).click();
      const preview=page.getByRole('dialog',{name,exact:true});await preview.waitFor();
      if(name===photo.name)assert(await preview.locator('.image-preview img').evaluate(img=>img.complete&&img.naturalWidth>0));
      else assert.match(await preview.locator('pre').innerText(),/最后一行。/);
      await page.keyboard.press('Escape');await preview.waitFor({state:'hidden'});
    }
    assert.equal(prompts().length,0);pass('file selection shows decodable image and full safe text previews without uploading');

    fixture.controls.promptDelayMs=1000;
    await send().click();
    await page.waitForFunction(()=>Boolean(document.querySelector('.pending-user-message .conversation-message-image')));
    assert.equal(prompts().length,1);
    const first=prompts()[0].body;
    assert.equal(first.message,undefined,'images use ordered native blocks, not a fake text-only send');
    assert.equal(first.contentBlocks[1].mimeType,'image/png');
    assert.equal(first.contentBlocks[1].data,samples.small,'actual raster bytes reach the node');
    assert(first.contentBlocks[0].text.includes(text.buffer.toString()));
    assert(first.contentBlocks[0].text.includes(photo.name));
    assert(await input().isDisabled(),'cannot mutate pending attachment selection');
    await field().fill('发送过程中继续编辑的新草稿');
    await page.waitForFunction(()=>document.querySelectorAll('.draft-attachments .draft-chip').length===0);
    assert.equal(await field().inputValue(),'发送过程中继续编辑的新草稿');
    fixture.controls.promptDelayMs=0;
    await settle(first,1);
    const sentImage=page.locator('[data-history-id="attachment-user-1"] .conversation-message-image');
    await sentImage.waitFor();assert.equal(await sentImage.count(),1);
    pass('one real ordered payload contains image bytes and complete text; pending/sent image previews persist and ACK preserves a newer draft');

    await field().fill('失败时保留');await choose([photo]);await ready();
    fixture.controls.failPrompt=true;await send().click();
    await page.locator('.composer-foot [role=status]').filter({hasText:'未被节点接受'}).waitFor();
    assert.equal(await chips().count(),1);assert.equal(await field().inputValue(),'失败时保留');
    assert.equal(prompts().length,2);
    fixture.controls.failPrompt=false;await field().fill('');await ready();
    await field().press('Enter');
    await page.waitForFunction(()=>document.querySelectorAll('.draft-chip').length===0);
    assert.equal(prompts().length,3);assert(prompts()[2].body.contentBlocks.some(b=>b.type==='image'));
    await settle(prompts()[2].body,3);
    pass('rejected submission preserves draft and attachment; explicit retry supports image-only Enter without duplicate requests');

    await choose([{name:'phone-photo.jpg',mimeType:'image/jpeg',buffer:Buffer.from(samples.large,'base64')}]);
    assert.match(await chips().innerText(),/压缩图片/);
    await ready();await send().click();await page.waitForFunction(()=>document.querySelectorAll('.draft-chip').length===0);
    const compressed=prompts().at(-1).body.contentBlocks.find(b=>b.type==='image');
    assert.equal(compressed.mimeType,'image/jpeg');assert(Buffer.from(compressed.data,'base64').length<=128*1024);
    await settle(prompts().at(-1).body,4);
    pass('oversized-dimension photo is explicitly represented as a bounded JPEG visual copy, never an omitted image');

    await field().fill('保留本地');await choose([photo]);
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.waitForFunction(()=>document.querySelector('.session-entry.selected')?.dataset.sessionId==='architecture');
    assert.equal(await chips().count(),0);
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.waitForFunction(()=>document.querySelector('.session-entry.selected')?.dataset.sessionId==='dark');
    assert.equal(await chips().count(),1);assert.equal(await field().inputValue(),'保留本地');
    const before=prompts().length;fixture.controls.promptContent=false;
    await ready();await send().click();await page.locator('.composer-foot [role=status]').filter({hasText:'unsupported'}).waitFor();
    assert.equal(prompts().length,before);assert.equal(await chips().count(),1);
    await page.getByRole('button',{name:`移除 ${photo.name}`,exact:true}).click();
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.locator('[data-session-id="dark"] .session-item').click();
    await choose([photo]);await ready();await send().click();
    await page.locator('.composer-foot [role=status]').filter({hasText:'不支持有序图片'}).waitFor();
    assert.equal(prompts().length,before);assert.equal(await chips().count(),1);
    await page.getByRole('button',{name:`移除 ${photo.name}`,exact:true}).click();
    await choose([text]);await field().fill('');await ready();await send().click();
    await page.waitForFunction(()=>document.querySelectorAll('.draft-chip').length===0);
    assert.equal(prompts().at(-1).body.contentBlocks,undefined);
    assert(prompts().at(-1).body.message.includes(text.buffer.toString()));
    await settle(prompts().at(-1).body,5);
    pass('node/session switching isolates attachments; lost/absent image capability fails closed while complete text attachments still work on old nodes');

    // A running native turn supports text queues only. Never downgrade images.
    fixture.emit({type:'agent_start',runId:'queue-image-guard'});
    await field().fill('运行中图片保留');await choose([photo]);
    await page.getByRole('button',{name:'停止运行',exact:true}).waitFor();
    const queuedBefore=fixture.requests.filter(r=>r.body?.type==='queue_message').length;
    await field().press('Enter');await page.locator('.composer-foot [role=status]').filter({hasText:'未排队'}).waitFor();
    assert.equal(fixture.requests.filter(r=>r.body?.type==='queue_message').length,queuedBefore);
    const abortBefore=fixture.requests.filter(r=>r.body?.type==='abort').length;
    await page.getByRole('button',{name:'停止运行',exact:true}).click();
    await page.waitForFunction(()=>document.querySelector('.send-button')?.disabled);
    assert.equal(fixture.requests.filter(r=>r.body?.type==='abort').length,abortBefore+1);
    assert.equal(await chips().count(),1);assert.equal(await field().inputValue(),'运行中图片保留');
    fixture.emit({type:'agent_settled',runId:'queue-image-guard',outcome:'interrupted',pendingResume:false});await ready();
    await page.getByRole('button',{name:`移除 ${photo.name}`,exact:true}).click();
    fixture.emit({type:'agent_start',runId:'queue-text-fixture'});
    await choose([text]);await field().fill('排队完整文本');await page.getByRole('button',{name:'停止运行',exact:true}).waitFor();await field().press('Enter');
    await page.waitForFunction(()=>document.querySelectorAll('.draft-chip').length===0);
    const queued=fixture.requests.filter(r=>r.body?.type==='queue_message');assert.equal(queued.length,queuedBefore+1);assert(queued.at(-1).body.message.includes(text.buffer.toString()));assert.equal(queued.at(-1).body.contentBlocks,undefined);
    fixture.emit({type:'agent_settled',runId:'queue-text-fixture',outcome:'completed',pendingResume:false});await send().waitFor();
    pass('busy Enter refuses image queues; Stop only aborts and preserves image/draft; complete text attachments queue on explicit acceptance');

    await page.evaluate(()=>{const read=FileReader.prototype.readAsArrayBuffer;FileReader.prototype.readAsArrayBuffer=function(file){if(file.name!=='slow.txt')return read.call(this,file);const reader=this;window.__releaseAttachmentRead=()=>{FileReader.prototype.readAsArrayBuffer=read;read.call(reader,file);delete window.__releaseAttachmentRead;};};});
    const loadingCount=prompts().length;
    await input().setInputFiles({name:'slow.txt',mimeType:'text/plain',buffer:Buffer.from('slow complete file')});
    await page.locator('.attachment-files-slot [role=status]').filter({hasText:'正在读取'}).waitFor();
    await field().fill('读取期间不发送');await field().press('Enter');await page.locator('.composer-foot [role=status]').filter({hasText:'附件仍在读取'}).waitFor();assert.equal(prompts().length,loadingCount);
    await page.locator('[data-session-id="architecture"] .session-item').click();
    await page.waitForFunction(()=>document.querySelector('.session-entry.selected')?.dataset.sessionId==='architecture');
    await page.evaluate(()=>window.__releaseAttachmentRead());
    await page.locator('[data-session-id="dark"] .session-item').click();
    await page.locator('.attachment-files-slot [role=alert]').filter({hasText:'附件读取已中断'}).waitFor();
    assert.equal(await chips().count(),0);await ready();await send().click();await page.locator('.composer-foot [role=status]').filter({hasText:'附件读取或预览存在错误'}).waitFor();assert.equal(prompts().length,loadingCount);
    await page.getByRole('button',{name:'清除提示，仅保留已加入的附件',exact:true}).click();
    const nine=Array.from({length:9},(_,i)=>({name:`batch-${i}.txt`,mimeType:'text/plain',buffer:Buffer.from('batch text')}));
    await input().setInputFiles(nine);await page.locator('.attachment-files-slot [role=alert]').filter({hasText:'超出附件限制'}).waitFor();
    assert.equal(await chips().count(),8);await ready();await send().click();await page.locator('.composer-foot [role=status]').filter({hasText:'附件读取或预览存在错误'}).waitFor();assert.equal(prompts().length,loadingCount);
    await page.getByRole('button',{name:'清除提示，仅保留已加入的附件',exact:true}).click();
    for(let i=0;i<8;i++)await page.getByRole('button',{name:`移除 batch-${i}.txt`,exact:true}).click();
    pass('first FileReader load blocks sending; navigation interrupts it safely; partially rejected or interrupted selections require explicit acknowledgement');

    const binary={name:'document.pdf',mimeType:'application/pdf',buffer:Buffer.from('%PDF-1.0 binary document')};
    await choose([binary]);await field().fill('不要漏掉附件');await ready();const count=prompts().length;await send().click();
    await page.locator('.composer-foot [role=status]').filter({hasText:'暂不支持 PDF'}).waitFor();assert.equal(prompts().length,count);assert.equal(await chips().count(),1);
    assert.match(await chips().innerText(),/暂不支持/);
    await page.getByRole('button',{name:'移除 document.pdf',exact:true}).click();
    await choose([{name:'large.txt',mimeType:'text/plain',buffer:Buffer.from('中'.repeat(12000))}]);await ready();await send().click();
    await page.locator('.composer-foot [role=status]').filter({hasText:'32 KiB'}).waitFor();assert.equal(prompts().length,count);assert.equal(await chips().count(),1);
    await page.getByRole('button',{name:'移除 large.txt',exact:true}).click();
    fixture.controls.promptContent=PROFILE;
    await page.locator('[data-session-id="architecture"] .session-item').click();await page.locator('[data-session-id="dark"] .session-item').click();
    await choose([{name:'broken.png',mimeType:'image/png',buffer:Buffer.from([137,80,78,71,13,10,26,10,1])}]);await ready();await send().click();
    await page.locator('.composer-foot [role=status]').filter({hasText:/无法解码|附件读取或预览存在错误/}).waitFor();assert.equal(prompts().length,count);assert.match(await chips().innerText(),/图片读取失败/);
    await page.getByRole('button',{name:'移除 broken.png',exact:true}).click();
    await page.evaluate(()=>{window.__attachmentDecodeCount=0;const decode=HTMLImageElement.prototype.decode;HTMLImageElement.prototype.decode=function(){window.__attachmentDecodeCount++;return decode.call(this);};window.__restoreAttachmentDecode=()=>{HTMLImageElement.prototype.decode=decode;delete window.__restoreAttachmentDecode;};});
    const bomb=Buffer.alloc(33);Buffer.from([137,80,78,71,13,10,26,10]).copy(bomb);bomb.writeUInt32BE(13,8);bomb.write('IHDR',12);bomb.writeUInt32BE(50000,16);bomb.writeUInt32BE(50000,20);
    await choose([{name:'excessive-pixels.png',mimeType:'image/png',buffer:bomb}]);
    assert.equal(await page.evaluate(()=>window.__attachmentDecodeCount),0,'reject huge pixel headers before allocating a browser decoder');assert.equal(await chips().locator('img').count(),0);
    await ready();await send().click();await page.locator('.composer-foot [role=status]').filter({hasText:'像素过大'}).waitFor();assert.equal(prompts().length,count);
    await page.evaluate(()=>window.__restoreAttachmentDecode());await page.getByRole('button',{name:'移除 excessive-pixels.png',exact:true}).click();
    pass('unsupported PDF, oversized complete text, corrupt images and excessive pixel headers block the entire send without silent omission, truncation or unbounded browser decode');

    fixture.controls.promptContent=PROFILE;
    await page.locator('[data-session-id="architecture"] .session-item').click();await page.locator('[data-session-id="dark"] .session-item').click();
    await choose([photo,text]);
    for(const mode of ['light','dark']) {
      await page.getByRole('button',{name:'外观与主题',exact:true}).click();const theme=page.getByRole('dialog',{name:'外观与主题',exact:true});await theme.getByLabel('明暗模式',{exact:true}).selectOption(mode);await theme.getByRole('button',{name:'保存',exact:true}).click();await theme.waitFor({state:'hidden'});
      for(const width of [1440,390,320]) {
        await page.setViewportSize({width,height:width===1440?1000:844});
        assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no viewport overflow');
        for(const selector of ['.draft-chip>button:first-child','.attachment-remove','.extras-add']) for(const button of await page.locator(selector).all()) {const box=await button.boundingBox();assert(box&&box.width>=44&&box.height>=44,`${selector} touch area`);}
        await page.getByRole('button',{name:`预览 ${photo.name}`,exact:true}).click();const preview=page.getByRole('dialog',{name:photo.name,exact:true});await preview.waitFor();assert(await preview.evaluate(el=>el.scrollWidth<=el.clientWidth+1));
        await page.screenshot({path:`artifacts/attachment-send-${mode}-${width}.png`,animations:'disabled'});geometry.push({mode,width});await page.keyboard.press('Escape');await preview.waitFor({state:'hidden'});
      }
      await page.setViewportSize({width:1440,height:1000});
    }
    assert.deepEqual(errors,[]);pass('1440/390/320 light/dark previews and 44px selection/removal targets fit without overflow or application errors');
  } catch(error) { errors.push(error.stack);await page.screenshot({path:'artifacts/attachment-send-failure.png'}).catch(()=>{});throw error; }
  finally {fs.writeFileSync('artifacts/attachment-send-validation.json',JSON.stringify({results,errors,geometry,prompts:prompts().map(r=>({path:r.path,text:r.body.message||r.body.contentBlocks?.filter(b=>b.type==='text').map(b=>b.text).join(''),images:r.body.contentBlocks?.filter(b=>b.type==='image').map(b=>({mime:b.mimeType,bytes:Buffer.from(b.data,'base64').length}))}))},null,2));await browser.close();await fixture.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
