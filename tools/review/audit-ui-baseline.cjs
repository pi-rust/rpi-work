const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require('../../tests/support/browser.cjs');
(async()=>{
  const native = await chromium.connectOverCDP('http://127.0.0.1:9223');
  const page = native.contexts()[0].pages()[0];
  const browser = await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const reference = await browser.newPage();
  const result = {};
  for (const [name,p] of [['implementation',page],['reference',reference]]) {
    if(name==='reference') await p.goto('http://127.0.0.1:4182/prototype/');
    result[name]={};
    for(const [label,width,height] of [['desktop',1440,1000],['mobile',390,844]]) {
      await p.setViewportSize({width,height});
      try { await p.screenshot({path:`artifacts/audit-before-${name}-${label}.png`,timeout:3000}); }
      catch(error) { console.log(`Screenshot unavailable for ${name}: ${error.message.split('\n')[0]}`); }
      result[name][label]=await p.evaluate(()=>({
        width:innerWidth,height:innerHeight,scrollWidth:document.documentElement.scrollWidth,
        regions:[...document.querySelectorAll('.sidebar,.main-column,.workspace,.app-header,.topbar,.transcript,.task-body,.composer-main,.composer,.inspector,.scrim')].map(e=>{
          const r=e.getBoundingClientRect(),s=getComputedStyle(e);
          return {selector:e.className,x:r.x,y:r.y,w:r.width,h:r.height,display:s.display,position:s.position};
        }),
        text:document.body.innerText.slice(0,10000),
      }));
    }
  }
  fs.writeFileSync('artifacts/ui-baseline-audit.json',JSON.stringify(result,null,2));
  console.log(JSON.stringify(result,null,2));
  await browser.close();
  await native.close();
})().catch(e=>{console.error(e);process.exit(1)});
