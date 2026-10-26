const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const manifest=(id='review.owner-a',version='0.1.0',name='Roundtrip Package')=>({schemaVersion:1,kind:'rpi-work-design-package',id,name,version,author:'QA',description:'本地设计资源往返验证',resources:{skills:[{id:'contract-review',name:'Owner A Review',description:'审阅接口',content:'# Owner A\n\n保留来源和正文。'}],tools:['read'],prompts:[{id:'review',name:'Review template',kind:'template',content:'请审阅当前变更。'}]},design:{resourceKind:'Package',sources:{'README.md':'immutable source 0.1.0'}},testSnapshot:{revision:1,version,sources:{'README.md':'immutable source 0.1.0'},passed:true},extraMetadata:{preserve:true}});
(async()=>{
  const browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
  const page=await browser.newPage({viewport:{width:1100,height:950}});page.setDefaultTimeout(7000);
  const errors=[],results=[];page.on('pageerror',e=>errors.push(e.message));
  const pass=name=>{results.push(name);console.log('PASS',name)};
  async function importPackage(value){await page.getByRole('button',{name:/导入资源包/}).click();const m=page.getByRole('dialog',{name:'导入设计包',exact:true});await m.locator('textarea').fill(JSON.stringify(value));await m.getByRole('button',{name:'校验并导入',exact:true}).click();if(await m.isVisible()&&/已读取|已导入/.test(await m.innerText()))await page.keyboard.press('Escape');return m;}
  const card=(name,version)=>page.locator('.resource-card').filter({hasText:name}).filter({hasText:version});
  async function exportCard(name,version){const [download]=await Promise.all([page.waitForEvent('download'),card(name,version).getByRole('button',{name:'导出 JSON',exact:true}).click()]);return JSON.parse(fs.readFileSync(await download.path(),'utf8'));}
  try{
    await page.goto('http://127.0.0.1:4173/target/parity-modules-all-web/');
    await page.getByRole('button',{name:'packages',exact:true}).click();
    const original=manifest();
    await importPackage(original);await card('Roundtrip Package','0.1.0').waitFor();
    assert.deepEqual(await exportCard('Roundtrip Package','0.1.0'),original);
    pass('import/export preserves full manifest, design sources, snapshots, and metadata');
    const conflicting=structuredClone(original);conflicting.design.sources['README.md']='conflicting replacement';
    const conflictDialog=await importPackage(conflicting);
    await conflictDialog.locator('.resource-error').waitFor();
    await conflictDialog.getByRole('button',{name:'取消',exact:true}).click();
    assert.deepEqual(await exportCard('Roundtrip Package','0.1.0'),original);
    pass('different source for existing id@version is rejected without corrupting stored release');
    await page.getByRole('button',{name:'打开会话资源配置',exact:true}).click();
    let config=page.getByRole('dialog',{name:'会话资源配置',exact:true});
    await config.locator('.session-resource-row').filter({hasText:'Roundtrip Package'}).filter({hasText:'0.1.0'}).getByRole('button',{name:'用于本会话',exact:true}).click();
    await page.keyboard.press('Escape');
    await importPackage(manifest('review.owner-a','0.2.0'));
    assert.match(await page.locator('.session-resource-caption').innerText(),/@0\.1\.0/);
    await page.getByRole('button',{name:'Session B',exact:true}).click();
    assert.match(await page.locator('.session-resource-caption').innerText(),/未绑定/);
    await page.getByRole('button',{name:'Session A',exact:true}).click();
    assert.match(await page.locator('.session-resource-caption').innerText(),/@0\.1\.0/);
    pass('new versions do not rebind a session and bindings stay isolated by session');
    const ownerB=manifest('review.owner-b','0.1.0','Other Owner');ownerB.resources.skills[0].name='Owner B Review';ownerB.resources.skills[0].content='# Owner B\n\n其他来源。';await importPackage(ownerB);
    await page.getByRole('button',{name:'skills',exact:true}).click();
    await page.locator('.resource-card').filter({hasText:'Owner A Review'}).first().waitFor();
    await page.locator('.resource-card').filter({hasText:'Owner B Review'}).waitFor();
    pass('same skill id from multiple packages/versions keeps distinct owners');
    await page.getByRole('button',{name:'packages',exact:true}).click();
    const extension=manifest('review.extension','0.1.0','Review Extension');extension.design={resourceKind:'Extension',sources:{'src/lib.rs':'// unexecuted extension source'}};extension.resources={skills:[],tools:[],prompts:[]};await importPackage(extension);
    await page.getByRole('button',{name:'extensions',exact:true}).click();
    const extensionCard=page.locator('.resource-card').filter({hasText:'Review Extension'});await extensionCard.waitFor();
    assert.match(await extensionCard.innerText(),/待节点安装|等待节点安装/);
    const extDownload=page.waitForEvent('download');await extensionCard.getByRole('button',{name:'导出 JSON',exact:true}).click();
    assert.deepEqual(JSON.parse(fs.readFileSync(await (await extDownload).path(),'utf8')),extension);
    pass('Extension design is classified correctly and remains uninstalled on node');
    await page.reload();await page.getByRole('button',{name:'packages',exact:true}).click();
    await card('Roundtrip Package','0.1.0').waitFor();
    assert.match(await page.locator('.session-resource-caption').innerText(),/@0\.1\.0/);
    pass('personal library and session versions persist after reload');
    for(const width of [390,320]){await page.setViewportSize({width,height:844});await page.screenshot({path:`artifacts/ui-resources-module-${width}.png`});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.getByRole('button',{name:'打开会话资源配置',exact:true}).click();await page.screenshot({path:`artifacts/ui-resources-module-config-${width}.png`});assert(await page.getByRole('dialog').evaluate(e=>e.scrollWidth<=e.clientWidth+1));await page.keyboard.press('Escape');}
    pass('390/320 resource pages and configuration dialogs stay bounded');
    assert.deepEqual(errors,[]);pass('no application JavaScript errors');
    fs.writeFileSync('artifacts/resources-module-validation.json',JSON.stringify({scope:'test-only Dioxus module harness; not full app integration',results,errors},null,2));
  }catch(error){await page.screenshot({path:'artifacts/ui-resources-module-failure.png'});fs.writeFileSync('artifacts/resources-module-validation.json',JSON.stringify({results,errors,fatal:error.message},null,2));throw error;}
  finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
