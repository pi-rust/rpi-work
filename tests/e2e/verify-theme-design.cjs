// Real Dioxus/WASM styling; the HTTP node is a local fixture, never a model call.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { chromium } = require('../support/browser.cjs');
const { startFixture } = require('../fixtures/ui-fixture.cjs');
const url = process.env.WORK_UI_URL || 'http://127.0.0.1:4182/app/';
const sample = JSON.parse(fs.readFileSync('examples/themes/codex-reading.json', 'utf8'));
const custom = structuredClone(sample);
custom.theme.id = 'visual-proof'; custom.theme.name = '完整视觉验证';
custom.theme.design.geometry.radii = [0,2,4,6,8]; custom.theme.design.geometry.borderWidth = 2;
custom.theme.design.shadow = 'none';

(async () => {
  fs.mkdirSync('artifacts', { recursive: true });
  const fixture = await startFixture(0), browser = await chromium.launch({ headless:true, executablePath:process.env.CHROME_PATH });
  const page = await browser.newPage({ viewport:{ width:1440, height:1000 } });
  const results = [], errors = []; page.setDefaultTimeout(10000); page.on('pageerror', error => errors.push(error.message));
  const pass = text => { results.push(text); console.log('PASS', text); };
  const dialog = () => page.getByRole('dialog', { name:'外观与主题', exact:true });
  const open = () => page.getByRole('button', { name:'外观与主题', exact:true }).click();
  const importTheme = value => dialog().locator('#appearance-import-file').setInputFiles({ name:'visual.json', mimeType:'application/json', buffer:Buffer.from(JSON.stringify(value)) });
  const active = theme => page.waitForFunction(id => document.documentElement.dataset.theme === id, theme);
  const near = (actual, expected) => assert(Math.abs(parseFloat(actual)-expected)<0.1, `${actual} vs ${expected}`);
  const computed = (selector, property) => page.locator(selector).first().evaluate((element, property) => getComputedStyle(element).getPropertyValue(property), property);
  const capture = async path => { await page.evaluate(async () => Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect?.getTiming().iterations)).map(a => a.finished.catch(()=>{})))); await page.screenshot({path}); };
  try {
    await page.goto(url); await active('codex');
    await page.getByRole('button', {name:'打开设置',exact:true}).click();
    await page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await page.getByRole('button',{name:'连接其他节点',exact:true}).click();
    await page.getByLabel('节点地址',{exact:true}).fill(fixture.url); await page.getByRole('button',{name:'连接节点',exact:true}).click();
    await page.getByRole('status').filter({hasText:'在线'}).waitFor(); await page.getByRole('button',{name:'返回会话',exact:true}).click();
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await page.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).waitFor();
    fixture.emit({type:'message_end',message:{entryId:'visual-code',role:'assistant',content:[{type:'text',text:'```rust\nfn visual_theme() {}\n```'}]}});
    await page.locator('.conversation-code pre').waitFor();
    fixture.emit({type:'agent_settled',outcome:'completed',pendingResume:false});
    await page.locator('.composer textarea').fill('视觉规格切换不能丢失这份草稿');

    await open(); await importTheme(sample); await active(sample.theme.id);
    const [sampleDownload] = await Promise.all([page.waitForEvent('download'),dialog().getByRole('button',{name:'导出主题 JSON',exact:true}).click()]);
    assert.deepEqual(JSON.parse(fs.readFileSync(await sampleDownload.path(),'utf8')),sample);
    pass('the checked-in complete v2 theme imports and exports without losing any visual specification');
    await importTheme(custom); await active(custom.theme.id);
    assert.match(await computed('body','font-family'),/Georgia/);
    assert.match(await computed('.app-header h1','font-family'),/Segoe UI/);
    assert.match(await computed('.conversation-code pre','font-family'),/Cascadia Code/);
    near(await computed('body','font-size'),17.6);
    near(await computed('.conversation-user-text','font-size'),17.6);
    near(await computed('.conversation-user-text','line-height'),32.56);
    near(await computed('.conversation-code pre','font-size'),13.2);
    near(await computed('.app-header h1','font-size'),22);
    assert.equal(await computed('.app-header h1','font-weight'),'600');
    near(await computed('.conversation-widget.user-message .message-content','padding-left'),11);
    assert.equal(await computed('#appearance','border-radius'),'6px');
    assert.equal(await computed('#appearance','border-top-width'),'2px');
    assert.equal(await computed('#appearance','box-shadow'),'none');
    assert.equal(await computed('.composer-main .composer','box-shadow'),'none');
    assert.equal(await computed('.nav-item','height'),'44px');
    assert.match(await computed('.send-button','transition-duration'),/0\.1s/);
    assert.match(await page.getByRole('region',{name:'主题视觉规格'}).innerText(),/Georgia/);
    pass('font families, role sizes, line height, weight, spacing, radii, controls, border, shadow and motion affect actual elements');

    for(const [index,path,value] of [
      [0,['typography','bodyFont'],'url(https://fonts.invalid/font.woff2)'],
      [1,['typography','scale'],9], [2,['geometry','hitSize'],12],
      [3,['geometry','radii'],[9,1,2,3,4]], [4,['motion','normalMs'],3000],
      [5,['typography','css'],'body{display:none}'],
    ]) {
      const invalid=structuredClone(custom); invalid.theme.id=`unsafe-visual-${index}`;
      invalid.theme.design[path[0]][path[1]]=value; await importTheme(invalid);
      await dialog().locator('.theme-error').filter({hasText:/规格|字段|越界|动效|合法/}).waitFor();
      assert.equal(await page.evaluate(()=>document.documentElement.dataset.theme),custom.theme.id);
      assert.equal(await dialog().locator('.theme-preset').filter({hasText:'完整视觉验证'}).count(),1);
    }
    pass('external fonts, arbitrary CSS, invalid geometry and oversized numeric values reject atomically');
    await page.keyboard.press('Escape'); await dialog().waitFor({state:'hidden'}); await active('codex');
    near(await computed('body','font-size'),15); assert.match(await computed('body','font-family'),/Segoe UI/);
    assert.equal(await page.locator('.composer textarea').inputValue(),'视觉规格切换不能丢失这份草稿');
    pass('cancel restores every visual group and preserves the active draft');

    await open(); await importTheme(custom); await active(custom.theme.id);
    const [download] = await Promise.all([page.waitForEvent('download'),dialog().getByRole('button',{name:'导出主题 JSON',exact:true}).click()]);
    assert.deepEqual(JSON.parse(fs.readFileSync(await download.path(),'utf8')),custom);
    await dialog().getByRole('button',{name:'保存',exact:true}).click(); await dialog().waitFor({state:'hidden'});
    await capture('artifacts/theme-design-desktop.png');
    await page.reload(); await active(custom.theme.id); assert.match(await computed('body','font-family'),/Georgia/);
    await page.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await page.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    near(await computed('.conversation-user-text','font-size'),17.6);
    pass('v2 saved specifications survive export, persistence and a formal app reload');

    await open(); await dialog().getByLabel('界面密度',{exact:true}).selectOption('compact');
    near(await computed('.conversation-user-text','font-size'),16.5);
    near(await computed('.conversation-widget.user-message .message-content','padding-left'),9.46);
    const [compactDownload] = await Promise.all([page.waitForEvent('download'),dialog().getByRole('button',{name:'导出主题 JSON',exact:true}).click()]);
    assert.deepEqual(JSON.parse(fs.readFileSync(await compactDownload.path(),'utf8')).theme.design,custom.theme.design);
    await page.emulateMedia({reducedMotion:'reduce'}); assert.equal(await computed('.send-button','transition-duration'),'0s');
    pass('compact density composes with the design instead of modifying it; reduced-motion overrides imported durations');
    for(const width of [390,320]) {
      await page.setViewportSize({width,height:844});
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      assert(await dialog().evaluate(element=>element.scrollWidth<=element.clientWidth+1));
      assert(parseFloat(await computed('#color-mode','font-size'))>=16);
      assert(parseFloat(await computed('#appearance .modal-header .icon-button','height'))>=48);
      await capture(`artifacts/theme-design-mobile-${width}.png`);
    }
    pass('390/320px visual specifications remain bounded with input and touch safety floors');
    const largeTouch = structuredClone(custom);
    largeTouch.theme.id='visual-large-touch'; largeTouch.theme.design.geometry.hitSize=64;
    await importTheme(largeTouch); await active(largeTouch.theme.id);
    for(const width of [390,320]) {
      await page.setViewportSize({width,height:844});
      for(const button of await dialog().locator('.modal-actions > button').all()) {
        assert((await button.boundingBox()).height >= 64, JSON.stringify(await button.evaluate(element=>({classes:element.className,minHeight:getComputedStyle(element).minHeight,height:getComputedStyle(element).height,hit:getComputedStyle(element).getPropertyValue('--hit-size')}))));
      }
      const bodyBox = await dialog().locator(':scope > .modal-body').boundingBox();
      const footerBox = await dialog().locator(':scope > .modal-actions').boundingBox();
      assert(bodyBox.y + bodyBox.height <= footerBox.y + 1, 'footer overlaps scrolling body');
      assert(await dialog().evaluate(element=>element.scrollWidth<=element.clientWidth+1));
    }
    pass('maximum 64px touch targets stay outside the scrolling modal body at both phone widths');
    await page.setViewportSize({width:1440,height:1000});
    await dialog().locator('.theme-preset').filter({hasText:'Codex 黑白'}).click(); await active('codex');
    near(await computed('body','font-size'),15); assert.match(await computed('body','font-family'),/Segoe UI/);
    assert.equal(await computed('#appearance','border-radius'),'16px'); assert.equal(await computed('#appearance','border-top-width'),'1px');
    assert.notEqual(await computed('#appearance','box-shadow'),'none');
    assert.equal(await page.evaluate(()=>document.documentElement.style.getPropertyValue('--bg')),'');
    assert.deepEqual(errors,[]);
    pass('returning to a built-in clears v2 colors and typography/geometry/elevation without JavaScript errors');
    fs.writeFileSync('artifacts/theme-design-validation.json',JSON.stringify({results,errors,scope:'formal Dioxus; HTTP fixture; no external model'},null,2));
  } catch(error) { await page.screenshot({path:'artifacts/theme-design-failure.png'}); throw error; }
  finally { await browser.close(); await fixture.close(); }
})().catch(error=>{console.error(error);process.exitCode=1});
