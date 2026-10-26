const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'}),page=await browser.newPage({viewport:{width:1440,height:1000}});page.setDefaultTimeout(5000);
 const results=[],errors=[];page.on('pageerror',e=>errors.push(e.message));const pass=name=>{results.push(name);console.log('PASS',name)};
 try{
  await page.addInitScript(()=>{const start=window.setInterval.bind(window),stop=window.clearInterval.bind(window),active=new Set();window.__reviewModalIntervals=active;window.setInterval=(...args)=>{const id=start(...args);active.add(id);return id};window.clearInterval=id=>{active.delete(id);stop(id)}});
  await page.goto('http://127.0.0.1:4173/target/parity-web/');await page.getByRole('button',{name:'外观与主题',exact:true}).click();const appearance=page.getByRole('dialog',{name:'外观与主题',exact:true});await appearance.waitFor();await page.keyboard.press('Escape');await appearance.waitFor({state:'hidden'});await page.waitForFunction(()=>window.__reviewModalIntervals.size===0);pass('Escape releases modal focus-guard interval');
  await page.locator('.sidebar').getByRole('button',{name:'Packages',exact:true}).click();
  for(let i=0;i<3;i++){
   await page.getByRole('button',{name:/导入资源包/}).click();const dialog=page.getByRole('dialog',{name:'导入设计包',exact:true});await dialog.locator('textarea').fill(JSON.stringify({schemaVersion:1,kind:'rpi-work-design-package',id:`review.modal-${i}`,name:`Modal review ${i}`,version:'0.1.0',author:'QA',description:'Lifecycle verification',resources:{skills:[],tools:[],prompts:[]}}));await dialog.getByRole('button',{name:'校验并导入',exact:true}).click();await dialog.waitFor({state:'hidden'});await page.waitForFunction(()=>window.__reviewModalIntervals.size===0);
  }
  pass('automatic unmount after import releases every modal interval');assert.deepEqual(errors,[]);pass('no application JavaScript errors');fs.writeFileSync('artifacts/modal-lifecycle-validation.json',JSON.stringify({results,errors},null,2));
 }catch(e){const active=await page.evaluate(()=>window.__reviewModalIntervals?.size);fs.writeFileSync('artifacts/modal-lifecycle-validation.json',JSON.stringify({results,errors,activeIntervals:active,fatal:e.message},null,2));throw e;}
 finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
