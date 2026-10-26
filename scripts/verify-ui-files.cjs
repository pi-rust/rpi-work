const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {chromium}=require(require.resolve('playwright',{paths:[process.cwd(),path.resolve('../rpi-package/packages/rpi-web/frontend')]}));
const {startFixture}=require('./ui-fixture.cjs');
const results=[];
async function check(name,fn){try{await fn();results.push({name,passed:true});console.log('PASS',name)}catch(e){results.push({name,passed:false,error:e.message});console.log('FAIL',name,e.message.split('\n')[0])}}
(async()=>{
  const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:'C:/Users/bigfish/scoop/apps/googlechrome/current/chrome.exe'});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});page.setDefaultTimeout(5000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  try{
    await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4173/target/parity-web/');
    await page.getByRole('button',{name:'打开设置',exact:true}).click();
    await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await page.getByRole('button',{name:'连接节点',exact:true}).click();
    await page.getByRole('status').filter({hasText:'在线'}).waitFor();
    await page.getByRole('button',{name:'返回会话',exact:true}).click();
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).first().waitFor();
    await page.locator('.composer-main textarea').fill('文件预览必须保留本条草稿');
    await page.getByRole('button',{name:/^变更/}).click();
    await page.locator('.change-file').filter({hasText:'src/theme.rs'}).click();
    await check('change view shows patch with added/deleted lines and line numbers',async()=>{
      await page.locator('.change-preview').getByText(/let theme = "dark"/).waitFor();
      assert(await page.locator('.change-preview .diff-line.add').count()>0);
      assert(await page.locator('.change-preview .diff-line.del').count()>0);
      assert(await page.locator('.change-preview .line-number').count()>0);
    });
    await page.getByRole('button',{name:'打开项目文件',exact:true}).click();
    const modal=page.getByRole('dialog',{name:/项目文件/});
    await modal.locator('.project-file-row').filter({hasText:'README.md'}).click();
    await check('Markdown file preview renders structured content',async()=>{
      await modal.getByRole('heading',{name:'RPI Work',exact:true}).waitFor();
      assert.match(await modal.innerText(),/Rust \/ Dioxus/);
    });
    await check('HTML preview blocks scripts and resource/navigation requests',async()=>{
      await modal.locator('.project-file-row').filter({hasText:'preview.html'}).click();
      const frame=modal.frameLocator('iframe');
      await frame.getByRole('heading',{name:'项目预览',exact:true}).waitFor();
      await frame.getByText('外部链接',{exact:true}).click();
      assert.equal(await page.evaluate(()=>window.__previewExecuted),undefined);
      assert.equal(fixture.requests.filter(r=>r.path.startsWith('/probe-preview')).length,0);
      await frame.getByRole('heading',{name:'项目预览',exact:true}).waitFor();
    });
    await page.keyboard.press('Escape');
    await check('closing file browser restores the original diff and draft',async()=>{
      assert.match(await page.locator('.selected-file-heading').innerText(),/src\/theme.rs/);
      await page.getByRole('button',{name:'对话',exact:true}).click();
      assert.equal(await page.locator('.composer-main textarea').inputValue(),'文件预览必须保留本条草稿');
      await page.getByRole('button',{name:/^变更/}).click();
      assert.match(await page.locator('.selected-file-heading').innerText(),/src\/theme.rs/);
    });
    await check('deleted Git file still renders its patch',async()=>{
      await page.locator('.change-file').filter({hasText:'src/module-2.rs'}).click();
      await page.locator('.change-preview').getByText(/let theme = "dark"/).waitFor();
    });
    await page.getByRole('button',{name:'对话',exact:true}).click();
    await page.getByRole('button',{name:'打开项目文件',exact:true}).click();
    await modal.locator('.project-file-row').filter({hasText:'README.md'}).click();
    await check('file path quote appends to draft without sending a model prompt',async()=>{
      await modal.getByRole('button',{name:/引用.*(?:草稿|对话)|加入.*草稿/}).click();
      assert.match(await page.locator('.composer-main textarea').inputValue(),/文件预览必须保留本条草稿[\s\S]*README\.md/);
      assert.equal(fixture.requests.filter(r=>r.body.type==='prompt').length,0);
    });
    await check('no application JavaScript errors',async()=>assert.deepEqual(errors,[]));
    fs.writeFileSync('artifacts/ui-files-validation.json',JSON.stringify({results,errors,requests:fixture.requests},null,2));
    if(results.some(r=>!r.passed))process.exitCode=1;
  }finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
