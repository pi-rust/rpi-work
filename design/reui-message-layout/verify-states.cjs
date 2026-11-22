#!/usr/bin/env node
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../../tools/node_modules/playwright');
const url = process.argv[2] || 'http://127.0.0.1:4181/';
const out = process.env.RPI_LAYOUT_SHOTS || '/tmp/rpi-message-layout';
fs.mkdirSync(out,{recursive:true});
let count=0;
const pass=message=>{console.log('PASS '+message);count++};
(async()=>{
  const browser=await chromium.launch();
  try {
    const context=await browser.newContext({viewport:{width:1440,height:1000},permissions:['clipboard-read','clipboard-write']});
    const page=await context.newPage();
    const errors=[];const requests=[];
    page.on('pageerror',e=>errors.push(e.message));
    page.on('request',request=>requests.push({url:request.url(),method:request.method()}));
    const paint=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const select=async value=>{await page.getByLabel('预览消息状态',{exact:true}).selectOption(value);await paint()};
    const scrollMetrics=()=>page.evaluate(()=>({top:scrollArea.scrollTop,bottom:scrollArea.scrollHeight-scrollArea.clientHeight-scrollArea.scrollTop}));
    await page.goto(url);await page.evaluate(()=>{localStorage.clear();sessionStorage.clear()});await page.reload();await paint();
    assert.equal(await page.locator('.turn').count(),2);pass('completed preview preserves the two-turn layout');
    await page.locator('.answer').first().hover();await page.getByRole('button',{name:'复制回答',exact:true}).first().click();
    const fullAnswer=await page.evaluate(()=>navigator.clipboard.readText());assert.match(fullAnswer,/统一颜色来源/);assert.match(fullAnswer,/129 passed/);pass('answer copy includes full rendered prose instead of its summary');
    await page.locator('.answer').first().hover();await page.getByRole('button',{name:'引用回答',exact:true}).first().click();
    const quoted=await page.locator('#draft').inputValue();assert.match(quoted,/> • 统一颜色来源/);assert.match(quoted,/> 测试通过/);await page.locator('#draft').fill('');pass('quoting preserves multiline text and quotes each line');
    await page.evaluate(()=>bottom());await paint();assert(await page.locator('.jump-bottom').isHidden());pass('jump-to-latest hides at the bottom');
    await select('running');
    assert.equal(await page.locator('body').getAttribute('data-busy'),'true');
    assert(await page.getByRole('button',{name:'停止本地演示',exact:true}).isVisible());
    assert(await page.locator('.send-button').isHidden());pass('running shows activity and replaces send with stop');
    await page.getByRole('button',{name:'追加一段',exact:true}).click();await paint();
    assert.equal(await page.locator('#stream-output p').count(),1);assert((await scrollMetrics()).bottom<=2);pass('incremental output follows when already reading the bottom');
    await page.evaluate(()=>{scrollArea.scrollTop=0});await paint();const before=(await scrollMetrics()).top;
    await page.evaluate(()=>appendDemoChunk());await paint();
    assert(Math.abs((await scrollMetrics()).top-before)<1);assert(await page.locator('.jump-bottom.has-unread').isVisible());pass('new output preserves history position and exposes an unread jump');
    await page.getByRole('button',{name:'有新内容，回到最新消息',exact:true}).click();await paint();assert((await scrollMetrics()).bottom<=2);assert(await page.locator('.jump-bottom').isHidden());pass('unread jump resumes bottom-follow');
    await page.evaluate(()=>appendDemoChunk());await paint();assert((await scrollMetrics()).bottom<=2);pass('following stays enabled for the next output chunk');
    const paragraphs=await page.locator('#stream-output p').count();
    await page.locator('#draft').fill('执行中不应被快捷键提交');await page.locator('#draft').press('Control+Enter');
    assert.equal(await page.locator('.turn').count(),3);pass('busy composer cannot submit even via shortcut');
    await page.screenshot({path:path.join(out,'running-light.png')});
    await page.getByRole('button',{name:'停止本地演示',exact:true}).click();await paint();
    assert.equal(await page.locator('#stream-output p').count(),paragraphs);
    assert.match(await page.locator('.working-status').textContent(),/已停止/);
    assert.equal(await page.evaluate(()=>demoTimer),null);
    assert(await page.locator('.send-button').isVisible());pass('stop keeps generated output and restores the composer');
    await select('running');await page.getByRole('button',{name:'播放增量',exact:true}).click();
    await page.waitForFunction(()=>document.querySelectorAll('#stream-output p').length>=2);
    await page.getByRole('button',{name:'暂停增量',exact:true}).click();
    assert.equal(await page.evaluate(()=>demoTimer),null);pass('local stream playback can pause without losing content');
    await page.getByRole('button',{name:'完成本轮',exact:true}).click();
    assert.equal(await page.locator('body').getAttribute('data-busy'),'false');assert.match(await page.locator('.working-status').textContent(),/演示完成/);pass('explicit completion is distinct from command acceptance');
    await select('running');await page.getByRole('button',{name:'播放增量',exact:true}).click();await select('complete');
    assert.equal(await page.evaluate(()=>demoTimer),null);assert.equal(await page.locator('.scenario-turn').count(),0);pass('switching previews cancels playback and discards no static history');
    await select('failed');assert.match(await page.locator('.failed-tool').textContent(),/exit 1/);assert.equal(await page.locator('.failed-tool .activity-spinner').count(),0);assert(await page.locator('.error-notice').isVisible());
    await page.screenshot({path:path.join(out,'failed-light.png')});pass('failure retains exit code and diagnostic output without a success marker');
    await page.getByRole('button',{name:'模拟重试',exact:true}).click();assert.equal(await page.locator('body').getAttribute('data-busy'),'true');assert.equal(await page.getByLabel('预览消息状态',{exact:true}).inputValue(),'running');pass('simulated retry enters running instead of pretending success');
    await select('long');
    await page.locator('.scenario-turn').scrollIntoViewIfNeeded();await paint();
    assert.equal(await page.locator('.long-prose h3').count(),5);assert(await page.locator('.prose-table').isVisible());assert(await page.locator('.long-prose blockquote').isVisible());pass('long answer includes headings, list, quote, table, code and file references');
    const codeMetrics=await page.locator('.numbered-code').evaluate(el=>({scroll:el.scrollWidth,view:el.clientWidth}));assert(codeMetrics.scroll>codeMetrics.view);pass('long code line scrolls inside its own block');
    await page.getByRole('button',{name:'复制长代码',exact:true}).click();
    const copied=await page.evaluate(()=>navigator.clipboard.readText());assert(copied.startsWith('pub fn reading_column'));assert(!/^\d/m.test(copied));pass('code copy omits visual line numbers');
    await page.locator('.long-prose').evaluate(el=>{scrollArea.scrollTop+=el.getBoundingClientRect().top-scrollArea.getBoundingClientRect().top});await paint();await page.screenshot({path:path.join(out,'long-light.png')});
    await page.getByRole('button',{name:'切换深色模式',exact:true}).click();await page.screenshot({path:path.join(out,'long-dark.png')});
    const diffs=[];
    for (const name of ['theme.ts','settings-page.tsx','App.tsx']) {
      await page.evaluate(name=>openPreviewFile(name),name);await paint();
      diffs.push(await page.locator('.diff-table').textContent());
      assert.equal(await page.locator('.file-tree [aria-current="true"]').getAttribute('data-open-file'),name);
      assert.equal(await page.locator('.diff-table th').count(),4);
    }
    assert.equal(new Set(diffs).size,3);assert.match(diffs[1],/onCheckedChange/);assert.match(diffs[2],/useTheme/);pass('each selected file has its own diff and old/new line gutters');
    await page.getByRole('button',{name:'复制当前文件 diff',exact:true}).click();
    assert.match(await page.evaluate(()=>navigator.clipboard.readText()),/^--- a\/src\/App.tsx\n\+\+\+ b\/src\/App.tsx/);pass('diff copying corresponds to the selected file');
    await page.screenshot({path:path.join(out,'file-diff-dark.png')});
    // Every visual state must stay inside the desktop viewport with the rail open.
    for (const width of [1280,1440,1680]) {
      await page.setViewportSize({width,height:900});
      for (const state of ['complete','running','failed','long']) {
        await select(state);
        await page.evaluate(()=>setPanel('files'));await paint();
        const metrics=await page.evaluate(()=>{
          const column=document.querySelector('.reading-column').getBoundingClientRect();
          const composer=document.querySelector('.composer').getBoundingClientRect();
          const footer=document.querySelector('.statusbar').getBoundingClientRect();
          return {overflow:document.documentElement.scrollWidth>innerWidth,column:column.width,composer:composer.width,left:column.left-composer.left,obscured:composer.bottom>footer.top};
        });
        assert(!metrics.overflow,JSON.stringify({width,state,metrics}));assert(Math.abs(metrics.column-metrics.composer)<1);assert(Math.abs(metrics.left)<1);assert(!metrics.obscured);
      }
    }
    pass('all four previews fit 1280/1440/1680 desktops with aligned pinned composer');
    assert.deepEqual(errors,[]);pass('zero browser errors across state transitions');
    assert(requests.every(r=>new URL(r.url).origin===new URL(url).origin&&r.method==='GET'));pass('no external, model, node or mutation requests');
    console.log(`RESULT ${count} state checks passed; screenshots: ${out}`);
    await context.close();
  } finally { await browser.close(); }
})().catch(error=>{console.error('FAIL',error);process.exitCode=1});
