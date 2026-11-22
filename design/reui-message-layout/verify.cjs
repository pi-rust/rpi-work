#!/usr/bin/env node
// Run: node design/reui-message-layout/verify.cjs [http://127.0.0.1:4181/]
const assert = require('node:assert/strict');
const path = require('node:path');
const { chromium } = require('../../tools/node_modules/playwright');
const url = process.argv[2] || 'http://127.0.0.1:4181/';
const out = process.env.RPI_LAYOUT_SHOTS || '/tmp/rpi-message-layout';
const fs = require('node:fs');
fs.mkdirSync(out, { recursive:true });
let count = 0;
function pass(label) { count++; console.log('PASS ' + label); }
(async () => {
  const browser = await chromium.launch({ headless:true });
  try {
    const context = await browser.newContext({ viewport:{width:1440,height:1000}, permissions:['clipboard-read','clipboard-write'] });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror',e=>errors.push(e.message));
    await page.goto(url);
    await page.evaluate(()=>{localStorage.removeItem('rpi-message-layout-review-v2');sessionStorage.clear()});
    await page.reload();
    assert.equal(await page.locator('dialog[open]').count(),0); pass('settings no longer cover the transcript on entry');
    const rects = async () => page.evaluate(() => {
      const rect=s=>{const r=document.querySelector(s).getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height,right:r.right,bottom:r.bottom}};
      return {side:rect('.sidebar'),header:rect('.header'),column:rect('.reading-column'),composer:rect('.composer'),footer:rect('.statusbar'),bubble:rect('.user-bubble'),scroll:rect('.transcript-scroll'),rail:rect('.right-rail'),width:document.body.scrollWidth,viewport:innerWidth};
    });
    let g = await rects();
    assert.equal(g.side.w,264); assert.equal(g.header.h,44); assert.equal(g.footer.h,28); pass('new renderer shell: sidebar 264 / header 44 / footer 28');
    assert.equal(g.column.w,768); assert.equal(g.column.x,g.composer.x); assert.equal(g.column.w,g.composer.w); pass('message and composer share the exact 768px column');
    assert(g.bubble.x>g.column.x); assert.equal(g.bubble.right,g.column.right); pass('prompt is right-aligned inside the reading column');
    assert.equal(await page.locator('.turn').count(),2); assert.equal(await page.locator('.activity-summary[open]').count(),0); assert.equal(await page.locator('.tool-call[open]').count(),0); pass('two turns with collapsed activity and terminal output');
    await page.screenshot({path:path.join(out,'reading-light.png')});
    await page.locator('.activity-summary>summary').click();
    await page.locator('.tool-call').first().locator('summary').click();
    await page.getByText('palette.tokens()', {exact:false}).waitFor(); pass('activity group and individual tool output expand independently');
    await page.getByRole('button',{name:'复制工具输出',exact:true}).click();
    assert.match(await page.evaluate(()=>navigator.clipboard.readText()),/palette\.tokens/); pass('tool output copies to the clipboard');
    await page.getByRole('button',{name:'外观',exact:true}).click();
    assert(await page.getByRole('dialog',{name:'外观与布局'}).isVisible()); pass('appearance is a keyboard-accessible modal');
    await page.getByRole('button',{name:'经典',exact:true}).click();
    assert.equal(await page.locator('body').getAttribute('data-layout'),'classic');
    await page.keyboard.press('Escape');
    g = await rects(); assert.equal(g.bubble.x,g.column.x); assert.equal(g.bubble.w,g.column.w); pass('classic mode changes real message geometry');
    await page.screenshot({path:path.join(out,'classic-light.png')});
    await page.getByRole('button',{name:'外观',exact:true}).click();
    await page.getByRole('button',{name:'阅读',exact:true}).click();
    for (const label of ['轮次分隔','代码块通栏','深色模式']) await page.getByRole('switch',{name:label,exact:true}).click();
    await page.getByRole('switch',{name:'消息悬浮操作',exact:true}).click();
    await page.getByRole('switch',{name:'紧凑活动视图',exact:true}).click();
    await page.keyboard.press('Escape');
    const style = await page.evaluate(()=>({
      divider:getComputedStyle(document.querySelector('.turn+.turn')).borderTopWidth,
      wide:document.querySelector('.wide-content').getBoundingClientRect().width,
      column:document.querySelector('.reading-column').getBoundingClientRect().width,
      actions:getComputedStyle(document.querySelector('.message-actions')).display,
      toolVisible:document.querySelector('.activity-item').checkVisibility({contentVisibilityAuto:true}),
      activityOpen:document.querySelector('.activity-summary').open,
      toolBorder:getComputedStyle(document.querySelector('.tool-call')).borderTopWidth,
      bg:getComputedStyle(document.body).backgroundColor,
    }));
    assert.equal(style.divider,'1px'); pass('turn-divider setting draws an actual separator');
    assert.equal(style.wide,style.column+48); pass('code bleed setting extends 24px on each side');
    assert.equal(style.actions,'none'); pass('hover-actions setting hides actual actions');
    assert(style.toolVisible); assert(style.activityOpen); assert.equal(style.toolBorder,'1px'); pass('non-compact activity setting exposes tool rows');
    assert.equal(style.bg,'rgb(25, 25, 25)'); pass('dark setting changes the actual palette');
    await page.reload();
    assert.equal(await page.locator('body').getAttribute('data-theme'),'dark');
    assert.equal(await page.locator('body').getAttribute('data-bleed'),'true');
    assert.equal(await page.locator('body').getAttribute('data-activity'),'false');
    assert.equal(await page.locator('.activity-summary[open]').count(),1); pass('appearance settings persist across reload');
    await page.getByRole('button',{name:'外观',exact:true}).click(); await page.screenshot({path:path.join(out,'settings-dark.png')});
    await page.getByRole('button',{name:'恢复默认',exact:true}).click(); await page.keyboard.press('Escape');
    await page.locator('.header [data-panel="run"]').click();
    g=await rects(); assert.equal(g.rail.w,300); assert.equal(g.column.x,g.composer.x); assert.equal(g.column.w,g.composer.w); pass('300px run rail preserves message/composer alignment');
    await page.screenshot({path:path.join(out,'reading-run-panel.png')});
    await page.getByRole('button',{name:'关闭详情面板',exact:true}).click();
    await page.locator('.change-card>summary').click(); await page.locator('.change-list [data-open-file="theme.ts"]').click();
    assert(await page.locator('#files-content').isVisible()); assert.equal(await page.locator('#diff-filename').textContent(),'theme.ts'); pass('changed-file summary opens the file diff panel');
    await page.getByRole('button',{name:'关闭详情面板',exact:true}).click();
    await page.getByRole('button',{name:'外观',exact:true}).click(); await page.getByRole('button',{name:'经典',exact:true}).click(); await page.keyboard.press('Escape'); await page.reload();
    assert.equal(await page.locator('body').getAttribute('data-layout'),'classic'); pass('layout selection itself persists');
    await page.getByRole('button',{name:'外观',exact:true}).click(); await page.getByRole('button',{name:'恢复默认',exact:true}).click(); await page.keyboard.press('Escape');
    await page.locator('.answer').first().hover(); await page.getByRole('button',{name:'引用回答',exact:true}).first().click();
    assert.match(await page.locator('#draft').inputValue(),/^> 已接入深色模式/); pass('quote action inserts the actual answer into the composer');
    await page.locator('#draft').fill('<img src=x onerror=alert(1)>');
    await page.getByRole('button',{name:'发送演示消息',exact:true}).click();
    assert.equal(await page.locator('.turn').count(),3); assert.equal(await page.locator('.user-bubble img').count(),0); assert.match(await page.locator('.user-bubble').last().textContent(),/<img/); pass('demo sending appends safe text, never executes user HTML');
    assert(await page.getByRole('button',{name:'发送演示消息',exact:true}).isDisabled()); pass('empty draft disables send');
    await page.locator('#draft').fill('中文输入未提交');
    await page.locator('#draft').dispatchEvent('keydown',{key:'Enter',metaKey:true,isComposing:true}); assert.equal(await page.locator('.turn').count(),3); pass('IME composition cannot trigger send');
    await page.keyboard.press('Control+k');
    assert(await page.locator('.search-dialog').isVisible()); await page.locator('#search').fill('RPC'); assert.equal(await page.locator('#search-results button').count(),1); await page.keyboard.press('Escape'); pass('command search filters local fixture sessions');
    for (const width of [1280,1440,1680]) {
      await page.setViewportSize({width,height:900});
      for (const panel of ['none','run','files']) {
        await page.evaluate(panel=>setPanel(panel),panel);
        g=await rects(); assert.equal(g.width,g.viewport); assert(g.composer.bottom<=g.footer.y); assert(Math.abs(g.composer.x-g.column.x)<1); assert(Math.abs(g.composer.w-g.column.w)<1);
      }
    }
    pass('1280/1440/1680px: both rail states fit without overflow or composer occlusion');
    await page.evaluate(()=>{localStorage.setItem('rpi-message-layout-review-v2','{bad json');}); await page.reload(); assert.equal(await page.locator('body').getAttribute('data-layout'),'codex'); pass('corrupt local settings fall back safely');
    assert.deepEqual(errors,[]); pass('zero browser page errors');
    console.log(`RESULT ${count} checks passed; screenshots: ${out}`);
    await context.close();
  } finally { await browser.close(); }
})().catch(e=>{console.error('FAIL',e);process.exitCode=1});
