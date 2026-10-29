const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('../../tests/support/browser.cjs');
const {startFixture}=require('../../tests/fixtures/ui-fixture.cjs');
(async()=>{
  const fixture=await startFixture(),browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  // Compare the same populated-queue state as the prototype. Empty queues in the
  // actual product deliberately collapse and must not be kept tall for this audit.
  fixture.setQueue([{entryId:'geometry-follow-up',queue:'followUp',status:'queued',text:'完成后检查手机端设置抽屉的对比度。'},{entryId:'geometry-steer',queue:'steer',status:'queued',text:'先保留现有快捷键，不要调整键位。'}]);
  try{
    const actual=await browser.newPage({viewport:{width:1440,height:1000}}),reference=await browser.newPage({viewport:{width:1440,height:1000}});
    await reference.goto('http://127.0.0.1:4182/prototype/');
    await actual.goto('http://127.0.0.1:4180/');
    await actual.getByRole('button',{name:'打开设置',exact:true}).click();
    await actual.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:'执行节点',exact:true}).click();
    await actual.getByLabel('节点地址',{exact:true}).fill(fixture.url);
    await actual.getByRole('button',{name:'连接节点',exact:true}).click();
    await actual.getByRole('status').filter({hasText:'在线'}).waitFor();
    await actual.getByRole('button',{name:'返回会话',exact:true}).click();
    await actual.locator('.project-row').filter({hasText:'rpi-work'}).click();
    await actual.locator('.sidebar-sessions').getByRole('button',{name:'为工作台添加深色模式',exact:true}).click();
    await actual.locator('.transcript').getByText('主题切换已经准备好，接下来运行组件测试。',{exact:true}).first().waitFor();
    const result={};
    const mapping={sidebar:['.sidebar','.sidebar'],header:['.app-header','.topbar'],toolbar:['.section-tabs','.conversation-toolbar'],inspector:['.inspector','.inspector'],inspectorTitle:['.inspector h3,.inspector .inspector-top','.inspector-top'],composerSurface:['.composer-main > .composer','.composer'],composerInput:['.composer-main textarea','.composer textarea'],timeline:['.transcript','.task-body'],queueShelf:['.queue-shelf','.queue-shelf']};
    const box=async(p,s)=>p.locator(s).first().evaluate(e=>{const r=e.getBoundingClientRect(),c=getComputedStyle(e);return {x:r.x,y:r.y,w:r.width,h:r.height,font:c.fontSize,lineHeight:c.lineHeight,padding:c.padding,border:c.borderRadius}});
    for(const [name,[a,r]] of Object.entries(mapping))result[name]={actual:await box(actual,a),reference:await box(reference,r)};
    await actual.getByRole('tab',{name:/^变更/}).click();
    await reference.getByRole('tab',{name:/变更/}).click();
    result.changeBrowser={actual:await box(actual,'.change-browser'),reference:await box(reference,'.change-browser')};
    result.changePreview={actual:await box(actual,'.change-preview'),reference:await box(reference,'.change-preview')};
    await actual.screenshot({path:'artifacts/ui-geometry-changes-actual.png'});
    await reference.screenshot({path:'artifacts/ui-geometry-changes-reference.png'});
    fs.writeFileSync('artifacts/ui-geometry-review.json',JSON.stringify(result,null,2));
    const differences=Object.fromEntries(Object.entries(result).filter(([,value])=>JSON.stringify(value.actual)!==JSON.stringify(value.reference)));
    console.log(JSON.stringify({regions:Object.keys(result).length,matched:Object.keys(result).length-Object.keys(differences).length,differences},null,2));
    assert.deepEqual(differences,{},'actual UI geometry differs from equivalent prototype state; see artifacts/ui-geometry-review.json');
  }finally{await browser.close();await fixture.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
