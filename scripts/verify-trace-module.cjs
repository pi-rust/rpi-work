const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
 const page=await browser.newPage({viewport:{width:1200,height:950}}); page.setDefaultTimeout(6000);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));
 const pass=name=>{results.push(name);console.log('PASS',name)};
 try{
  await page.goto('http://127.0.0.1:4173/target/parity-modules-trace-web/');
  await page.getByRole('button',{name:'traces',exact:true}).click();
  const open=()=>page.getByRole('button',{name:'查看会话追踪与耗时',exact:true}).click();
  await open();await page.getByRole('heading',{name:'此会话暂无追踪',exact:true}).waitFor();await page.keyboard.press('Escape');
  pass('empty trace opens native dialog and Escape closes it');
  for(const name of ['Start run','First text','Start tool','End tool','Agent end'])await page.getByRole('button',{name,exact:true}).click();
  await open();const dialog=page.getByRole('dialog',{name:'会话追踪',exact:true});
  await dialog.getByText('等待结算',{exact:true}).first().waitFor();
  assert(await dialog.locator('.trace-span-row').count()>=3);
  pass('real event hierarchy remains waiting after agent_end');
  await dialog.locator('.trace-span-row').filter({hasText:'read'}).click();
  await dialog.getByRole('button',{name:'输入 / 输出',exact:true}).click();
  const io=await dialog.locator('.trace-details').innerText();
  assert.match(io,/src\/main.rs/);assert.match(io,/public file excerpt/);assert.doesNotMatch(io,/test-should-be-redacted/);
  pass('tool input and output render with credentials redacted');
  await page.keyboard.press('Escape');await page.getByRole('button',{name:'Settle run',exact:true}).click();await open();
  await dialog.getByText('已完成',{exact:true}).first().waitFor();
  assert.match(await dialog.locator('.trace-details').innerText(),/public file excerpt/);
  const total=await dialog.locator('.trace-metrics > div').first().innerText();assert.match(total,/\d+(?:\.\d+)?\s*(?:ms|s|毫秒|秒)/);
  const metricText=await dialog.locator('.trace-metrics').innerText();assert.match(metricText,/—/);
  pass('settlement closes measured run and preserves selected tool; unreported usage stays unknown');
  for(const width of [390,320]){
   await page.setViewportSize({width,height:844});await page.screenshot({path:`artifacts/ui-trace-module-${width}.png`});
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
   const box=await dialog.boundingBox();assert(box.x>=0&&box.x+box.width<=width+1);
  }
  pass('390/320 trace dialog remains bounded');
  assert.deepEqual(errors,[]);pass('no application JavaScript errors');
  fs.writeFileSync('artifacts/trace-module-validation.json',JSON.stringify({scope:'test-only Dioxus module harness, not full integration',results,errors},null,2));
 }catch(e){await page.screenshot({path:'artifacts/ui-trace-module-failure.png'});fs.writeFileSync('artifacts/trace-module-validation.json',JSON.stringify({results,errors,fatal:e.message},null,2));throw e;}
 finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
