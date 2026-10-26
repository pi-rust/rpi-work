// Full-app acceptance only. Each mutation scenario owns its local test node.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{spawn}=require('node:child_process');
const root=process.cwd(),buildPath=path.join(root,'target/parity-web/build.json');
const build=JSON.parse(fs.readFileSync(buildPath,'utf8').replace(/^\uFEFF/,''));
const reference=JSON.parse(fs.readFileSync('artifacts/prototype-review-baseline.json','utf8').replace(/^\uFEFF/,''));
function changedSources(){return Object.entries(build.sourceHashes).filter(([name,hash])=>{const file=path.resolve(root,name);return !fs.existsSync(file)||crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex').toUpperCase()!==hash.toUpperCase()}).map(([name])=>name)}
function changedReference(){return Object.entries(reference.hashes).filter(([name,hash])=>!fs.existsSync(name)||crypto.createHash('sha256').update(fs.readFileSync(name)).digest('hex').toUpperCase()!==hash).map(([name])=>name)}
const checks=[
 ['shell-details','verify-ui-shell-details.cjs'],
 ['file-tree','verify-ui-file-tree.cjs'],
 ['queue-dialog','verify-ui-queue-dialog.cjs'],
 ['queue-proposals','verify-ui-queue-proposals.cjs'],
 ['provider-cards','verify-ui-provider-cards.cjs'],
 ['inspector','verify-ui-inspector.cjs'],
 ['multi-package','verify-ui-multi-package.cjs'],
 ['meta-editor','verify-ui-meta-editor.cjs'],
 ['meta-feedback','verify-ui-meta-feedback.cjs'],
 ['final-controls','verify-ui-final-controls.cjs'],
 ['authoring-context','verify-ui-authoring-context.cjs'],
 ['project-modal','verify-ui-project-modal.cjs'],
 ['btw-tools','verify-ui-btw-tools.cjs'],
 ['layout-theme','verify-ui-parity.cjs'],['behavior','verify-ui-behavior.cjs'],['auth-scope','verify-ui-auth-scope.cjs'],['streaming','verify-ui-streaming.cjs'],['files','verify-ui-files.cjs'],['resources-meta','verify-ui-resources-meta.cjs'],['navigation-trace','verify-ui-navigation-trace.cjs'],['project-alias','verify-ui-project-alias.cjs'],['conversation','verify-ui-conversation.cjs'],['modal-lifecycle','verify-modal-lifecycle.cjs'],['btw-scroll','verify-ui-btw-scroll.cjs'],['custom-theme','verify-theme-contract.cjs',true],['settings','verify-settings-module.cjs',true],['runtime-stats','verify-ui-runtime-stats.cjs'],['geometry','audit-ui-responsive-geometry.cjs'],['real-node-readonly','audit-real-node.cjs'],
];
async function run([name,script,formal]){
 const start=Date.now(),dir=path.join(root,'target/parity-suite');fs.mkdirSync(dir,{recursive:true});const logFile=path.join(dir,`${name}.log`);
 const result=await new Promise((resolve,reject)=>{let output='';const child=spawn(process.execPath,[path.join('scripts',script)],{cwd:root,windowsHide:true,env:{...process.env,...(formal?{WORK_UI_URL:'http://127.0.0.1:4173/target/parity-web/'}:{})},stdio:['ignore','pipe','pipe']});child.stdout.on('data',data=>output+=data);child.stderr.on('data',data=>output+=data);child.on('error',reject);child.on('exit',code=>{fs.writeFileSync(logFile,output);resolve({name,script,exitCode:code,passed:code===0,checks:(output.match(/^PASS /gm)||[]).length,elapsedMs:Date.now()-start,logFile})})});
 console.log(`${result.passed?'PASS':'FAIL'} ${name}: ${result.checks} checks`);if(!result.passed)console.error(fs.readFileSync(logFile,'utf8').slice(-2200));return result;
}
(async()=>{
 const before=[...changedSources(),...changedReference()];if(before.length)throw new Error(`Rebuild or reference review required: ${before.join(', ')}`);
 const results=[];for(let i=0;i<checks.length;i+=3)results.push(...await Promise.all(checks.slice(i,i+3).map(run)));
 const changed=[...changedSources(),...changedReference()],report={builtAt:build.builtAt,verifiedAt:new Date().toISOString(),wasmSha256:build.wasmSha256,profile:build.profile,scope:'formal Rust/Dioxus application; test-only fixture mutations; real node GET only',suites:results,checks:results.reduce((sum,r)=>sum+r.checks,0),geometryRegions:63,sourceChangesDuringVerification:changed,passed:results.every(r=>r.passed)&&changed.length===0};
 fs.writeFileSync('artifacts/ui-final-validation.json',JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,suites:results.length,checks:report.checks,geometryRegions:63,changed}));if(!report.passed)process.exitCode=1;
})().catch(error=>{console.error(error);process.exitCode=1});



