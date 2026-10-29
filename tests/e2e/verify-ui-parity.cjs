const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {chromium}=require('../support/browser.cjs');
const {startFixture}=require('../fixtures/ui-fixture.cjs');
const results=[],errors=[];
async function check(name,fn){try{await fn();results.push({name,passed:true});console.log('PASS',name)}catch(e){results.push({name,passed:false,error:e.message});console.log('FAIL',name,e.message)}}
(async()=>{
  const fixture=await startFixture();
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  try{
    page.on('pageerror',e=>errors.push(e.message));
    page.on('console',m=>{if(m.type()==='error')errors.push(m.text())});
    await page.goto(process.env.WORK_UI_URL||'http://127.0.0.1:4180/');
    await page.getByRole('button',{name:'打开设置',exact:true}).waitFor();
    await page.getByRole('button',{name:'打开设置',exact:true}).click();
    await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await page.getByRole('button',{name:'连接节点',exact:true}).click();
    await page.getByRole('status').filter({hasText:'在线'}).waitFor();
    await page.getByRole('button',{name:'返回会话',exact:true}).click();
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).waitFor();
    await check('connected UI restores real HTTP/SSE fixture history and tool cards',async()=>{
      assert.equal(await page.locator('.transcript details.tool-call').count(),2);
      assert(fixture.requests.some(r=>r.path.endsWith('/snapshot')));
      assert(fixture.requests.some(r=>r.path.endsWith('/events')));
    });
    await check('node configuration is located in settings',async()=>{
      assert.equal(await page.locator('.sidebar input[aria-label="节点地址"]').count(),0);
      assert.equal(await page.locator('.workspace > aside.panel').count(),0);
    });
    for(const [width,height] of [[1440,1000],[1280,800],[768,1024],[390,844],[320,740]]){
      await page.setViewportSize({width,height});
      await page.screenshot({path:`artifacts/ui-parity-${width}.png`,animations:'disabled'});
      if(width===768){
        await check('tablet does not automatically cover conversation with Inspector',async()=>{
          assert.equal(await page.locator('.inspector').isVisible(),false,'desktop-open Inspector becomes an unsolicited tablet overlay');
        });
        if(await page.locator('.inspector').isVisible())await page.getByRole('button',{name:'关闭任务上下文',exact:true}).click();
      }
      await check(`compact conversation and bounded layout at ${width}x${height}`,async()=>{
        const geometry=await page.evaluate(()=>{
          const box=s=>{const e=document.querySelector(s),r=e.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height,display:getComputedStyle(e).display}};
          return {width:innerWidth,height:innerHeight,scroll:document.documentElement.scrollWidth,header:box('.app-header'),tabs:box('.section-tabs'),timeline:box('.transcript'),composer:box('.composer-main')};
        });
        assert(geometry.scroll<=width+1,`horizontal overflow ${geometry.scroll} > ${width}`);
        assert(Math.abs(geometry.header.h-52)<1,`header height ${geometry.header.h}`);
        assert(Math.abs(geometry.tabs.h-44)<1,`toolbar height ${geometry.tabs.h}`);
        assert(Math.abs(geometry.timeline.y-(geometry.header.y+96))<2,`timeline starts at ${geometry.timeline.y}; expected ${geometry.header.y+96}`);
        assert(geometry.composer.y+geometry.composer.h<=height+1,'composer falls below viewport');
        assert(geometry.timeline.h>height*.35,`timeline too short: ${geometry.timeline.h}`);
      });
      if(width===390){
        await check('mobile project drawer opens, scrim closes it',async()=>{
          await page.getByRole('button',{name:'打开项目导航',exact:true}).click();
          await page.waitForFunction(()=>document.querySelector('.sidebar').getBoundingClientRect().x>=-1);
          const sidebar=await page.locator('.sidebar').boundingBox();
          assert(sidebar&&sidebar.x>=-1&&sidebar.width>=236);
          await page.getByRole('button',{name:'关闭项目导航',exact:true}).click({position:{x:380,y:100}});
          await page.waitForFunction(()=>document.querySelector('.sidebar').getBoundingClientRect().x<-250);
          assert((await page.locator('.sidebar').boundingBox()).x<0);
        });
        await check('mobile session picker opens and Escape restores it',async()=>{
          await page.getByRole('button',{name:'切换会话',exact:true}).click();
          await page.getByRole('dialog').waitFor();
          await page.keyboard.press('Escape');
          await page.getByRole('dialog').waitFor({state:'hidden'});
        });
      }
    }
    await page.setViewportSize({width:1440,height:1000});
    await check('Reddit dark theme applies and persists across reload',async()=>{
      await page.getByRole('button',{name:'外观与主题',exact:true}).click();
      await page.locator('.theme-preset').filter({hasText:'Reddit 橙'}).click();
      await page.getByLabel('明暗模式',{exact:true}).selectOption('dark');
      await page.getByRole('button',{name:'保存',exact:true}).click();
      const before=await page.evaluate(()=>({theme:document.documentElement.dataset.theme,mode:document.documentElement.dataset.mode,bg:getComputedStyle(document.body).getPropertyValue('--bg').trim()}));
      assert.equal(before.theme,'reddit');assert.equal(before.mode,'dark');assert.equal(before.bg,'#0e1113');
      await page.screenshot({path:'artifacts/ui-parity-reddit-dark.png'});
      await page.reload();
      await page.getByRole('button',{name:'外观与主题',exact:true}).waitFor();
      await page.waitForFunction(()=>document.documentElement.dataset.theme==='reddit'&&document.documentElement.dataset.mode==='dark');
    });
    await check('Escape cancels theme preview and restores saved choice',async()=>{
      await page.getByRole('button',{name:'外观与主题',exact:true}).click();
      await page.locator('.theme-preset').filter({hasText:'海湾蓝'}).click();
      await page.getByLabel('明暗模式',{exact:true}).selectOption('light');
      await page.keyboard.press('Escape');
      await page.getByRole('dialog').waitFor({state:'hidden'});
      await page.waitForFunction(()=>document.documentElement.dataset.theme==='reddit'&&document.documentElement.dataset.mode==='dark',null,{timeout:2000});
    });
    await check('no browser runtime errors',async()=>assert.deepEqual(errors,[]));
    const buildPath='target/parity-web/build.json';
    const build=fs.existsSync(buildPath)?JSON.parse(fs.readFileSync(buildPath,'utf8').replace(/^\uFEFF/,'')):null;
    fs.writeFileSync('artifacts/ui-parity-validation.json',JSON.stringify({fixture:'test-only HTTP/SSE; no real model',build,results,errors,requests:fixture.requests},null,2));
    if(results.some(r=>!r.passed))process.exitCode=1;
  }catch(error){
    await page.screenshot({path:'artifacts/ui-parity-failure.png'});
    console.log(await page.evaluate(()=>[...document.querySelectorAll('.workspace,.transcript,.conversation-panel,.queue,.section-tabs,.composer-main')].map(e=>({class:e.className,rect:JSON.stringify(e.getBoundingClientRect()),display:getComputedStyle(e).display}))));
    fs.writeFileSync('artifacts/ui-parity-validation.json',JSON.stringify({results,errors,fatal:error.message,requests:fixture.requests},null,2));
    throw error;
  }finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
