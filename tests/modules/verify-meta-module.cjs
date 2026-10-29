const fs=require('node:fs'),assert=require('node:assert/strict'),path=require('node:path');
const {chromium}=require('../support/browser.cjs');
(async()=>{
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const page=await browser.newPage({viewport:{width:1100,height:950}});page.setDefaultTimeout(7000);
  const errors=[],results=[];page.on('pageerror',e=>errors.push(e.message));
  const pass=name=>{results.push(name);console.log('PASS',name)};
  try{
    await page.goto('http://127.0.0.1:4182/modules/all/');
    await page.getByRole('button',{name:'+ 新建能力',exact:true}).click();
    await page.getByLabel('名称',{exact:true}).fill('独立审查包');
    await page.getByLabel('资源 ID',{exact:true}).fill('personal.review-package');
    await page.getByRole('button',{name:'创建设计草稿',exact:true}).click();
    await page.getByRole('heading',{name:'独立审查包',exact:true}).waitFor();
    pass('create personal Package design resource');
    assert.equal(await page.getByRole('button',{name:'发布个人版本',exact:true}).isDisabled(),true);
    await page.getByRole('button',{name:'结构校验',exact:true}).click();
    await page.getByRole('button',{name:'创建测试快照',exact:true}).waitFor({state:'visible'});
    await page.waitForFunction(()=>[...document.querySelectorAll('button')].some(b=>b.textContent==='创建测试快照'&&!b.disabled));
    await page.getByRole('button',{name:'创建测试快照',exact:true}).click();
    await page.waitForFunction(()=>[...document.querySelectorAll('button')].some(b=>b.textContent==='发布个人版本'&&!b.disabled));
    pass('browser structural validation and immutable test snapshot work without std clock panic');
    await page.getByRole('button',{name:'发布个人版本',exact:true}).click();
    await page.getByRole('button',{name:'继续迭代 · bump patch',exact:true}).waitFor();
    pass('successful publication creates one release');
    await page.getByRole('button',{name:'继续迭代 · bump patch',exact:true}).click();
    assert.match(await page.locator('.meta-resource-head').innerText(),/0\.1\.1/);
    assert.equal(await page.getByRole('button',{name:'发布个人版本',exact:true}).isDisabled(),true);
    await page.getByRole('button',{name:'编辑源码',exact:true}).click();
    await page.locator('.meta-files button').filter({hasText:'README.md'}).click();
    await page.locator('.meta-code').fill('# Changed after publication\n\n独立审查后的新源码。');
    await page.getByRole('button',{name:'保存草稿',exact:true}).click();
    assert.equal(await page.getByRole('button',{name:'发布个人版本',exact:true}).isDisabled(),true);
    pass('iteration bumps version and edited source invalidates current publish eligibility');
    await page.getByRole('button',{name:/个人资源库/}).click();
    const download=page.waitForEvent('download');
    await page.getByRole('button',{name:'导出设计 JSON',exact:true}).click();
    const file=await download;
    const exported=JSON.parse(fs.readFileSync(await file.path(),'utf8'));
    assert.equal(exported.version,'0.1.0');
    assert.doesNotMatch(exported.design.sources['README.md'],/Changed after publication/);
    assert.equal(exported.testSnapshot.version,'0.1.0');
    pass('published export keeps original source and original test snapshot');
    await page.reload();await page.getByRole('heading',{name:'元Agent',exact:true}).waitFor();
    await page.getByRole('heading',{name:'独立审查包',exact:true}).waitFor();
    assert.match(await page.locator('.meta-resource-head').innerText(),/0\.1\.1/);
    pass('personal resources persist after reload');
    for(const width of [390,320]){
      await page.setViewportSize({width,height:844});
      await page.screenshot({path:`artifacts/ui-meta-module-${width}.png`});
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      await page.getByRole('button',{name:'编辑源码',exact:true}).click();
      await page.screenshot({path:`artifacts/ui-meta-module-editor-${width}.png`});
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      await page.getByRole('button',{name:'返回资源列表',exact:true}).click();
    }
    pass('390/320 module layout and source editor remain bounded');
    assert.deepEqual(errors,[]);pass('no application JavaScript errors');
    fs.writeFileSync('artifacts/meta-module-validation.json',JSON.stringify({scope:'test-only Dioxus module harness; not full app integration',results,errors},null,2));
  }catch(error){await page.screenshot({path:'artifacts/ui-meta-module-failure.png'});fs.writeFileSync('artifacts/meta-module-validation.json',JSON.stringify({results,errors,fatal:error.message},null,2));throw error;}
  finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
