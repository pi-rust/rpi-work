// Full-app acceptance only. Each mutation scenario owns its local test node.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{spawn}=require('node:child_process');
const root=process.cwd(),buildPath=path.join(root,'target/parity-web/build.json');
const listing=process.argv.includes('--list');
const build=listing?null:JSON.parse(fs.readFileSync(buildPath,'utf8').replace(/^\uFEFF/,''));
const reference=JSON.parse(fs.readFileSync('tests/fixtures/baselines/prototype-review-baseline.json','utf8').replace(/^\uFEFF/,''));
function changedSources(){return Object.entries(build.sourceHashes).filter(([name,hash])=>{const file=path.resolve(root,name);return !fs.existsSync(file)||crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex').toUpperCase()!==hash.toUpperCase()}).map(([name])=>name)}
function changedReference(){return Object.entries(reference.hashes).filter(([name,hash])=>!fs.existsSync(name)||crypto.createHash('sha256').update(fs.readFileSync(name)).digest('hex').toUpperCase()!==hash).map(([name])=>name)}
const checks=[
 ['workspace-management','tests/e2e/verify-workspace-management.cjs'],
 ['reply-follow','tests/e2e/verify-reply-follow.cjs'],
 ['new-conversation','tests/e2e/verify-new-conversation.cjs'],
 ['shell-details','tests/design/verify-ui-shell-details.cjs'],
 ['file-tree','tests/e2e/verify-ui-file-tree.cjs'],
 ['queue-dialog','tests/e2e/verify-ui-queue-dialog.cjs'],
 ['queue-proposals','tests/e2e/verify-ui-queue-proposals.cjs'],
 ['provider-cards','tests/e2e/verify-ui-provider-cards.cjs'],
 ['resource-cleanup','tests/e2e/verify-ui-resource-cleanup.cjs'],
 ['inspector','tests/e2e/verify-ui-inspector.cjs'],
 ['multi-package','tests/e2e/verify-ui-multi-package.cjs'],
 ['meta-editor','tests/e2e/verify-ui-meta-editor.cjs'],
 ['meta-feedback','tests/e2e/verify-ui-meta-feedback.cjs'],
 ['final-controls','tests/e2e/verify-ui-final-controls.cjs'],
 ['authoring-context','tests/e2e/verify-ui-authoring-context.cjs'],
 ['project-modal','tests/e2e/verify-ui-project-modal.cjs'],
 ['btw-tools','tests/e2e/verify-ui-btw-tools.cjs'],
 ['layout-theme','tests/e2e/verify-ui-parity.cjs'],['behavior','tests/e2e/verify-ui-behavior.cjs'],['auth-scope','tests/e2e/verify-ui-auth-scope.cjs'],['streaming','tests/e2e/verify-ui-streaming.cjs'],['files','tests/e2e/verify-ui-files.cjs'],['resources-meta','tests/e2e/verify-ui-resources-meta.cjs'],['navigation-trace','tests/e2e/verify-ui-navigation-trace.cjs'],['project-alias','tests/e2e/verify-ui-project-alias.cjs'],['conversation','tests/e2e/verify-ui-conversation.cjs'],['modal-lifecycle','tests/e2e/verify-modal-lifecycle.cjs'],['btw-scroll','tests/e2e/verify-ui-btw-scroll.cjs'],['custom-theme','tests/e2e/verify-theme-contract.cjs',true],['settings','tests/modules/verify-settings-module.cjs',true],['runtime-stats','tests/e2e/verify-ui-runtime-stats.cjs'],['geometry','tools/review/audit-ui-responsive-geometry.cjs'],['real-node-readonly','tools/review/audit-real-node.cjs'],
];
async function run([name,script,formal]){
 const start=Date.now(),dir=path.join(root,'target/parity-suite');fs.mkdirSync(dir,{recursive:true});const logFile=path.join(dir,`${name}.log`);
 const result=await new Promise((resolve,reject)=>{let output='';const child=spawn(process.execPath,[path.resolve(root,script)],{cwd:root,windowsHide:true,env:{...process.env,...(formal?{WORK_UI_URL:'http://127.0.0.1:4180/'}:{})},stdio:['ignore','pipe','pipe']});child.stdout.on('data',data=>output+=data);child.stderr.on('data',data=>output+=data);child.on('error',reject);child.on('exit',code=>{fs.writeFileSync(logFile,output);resolve({name,script,exitCode:code,passed:code===0,checks:(output.match(/^PASS /gm)||[]).length,elapsedMs:Date.now()-start,logFile})})});
 console.log(`${result.passed?'PASS':'FAIL'} ${name}: ${result.checks} checks`);if(!result.passed)console.error(fs.readFileSync(logFile,'utf8').slice(-2200));return result;
}
if(listing){for(const [,script]of checks){if(!fs.existsSync(script))throw new Error(`Missing suite entry: ${script}`)}console.log(checks.map(([name,script])=>`${name}: ${script}`).join('\n'));}else (async()=>{
 const before=[...changedSources(),...changedReference()];if(before.length)throw new Error(`Rebuild or reference review required: ${before.join(', ')}`);
 const results=[];for(let i=0;i<checks.length;i+=3)results.push(...await Promise.all(checks.slice(i,i+3).map(run)));
 const changed=[...changedSources(),...changedReference()],report={builtAt:build.builtAt,verifiedAt:new Date().toISOString(),wasmSha256:build.wasmSha256,profile:build.profile,scope:'formal Rust/Dioxus application; test-only fixture mutations; real node GET only',suites:results,checks:results.reduce((sum,r)=>sum+r.checks,0),geometryRegions:63,sourceChangesDuringVerification:changed,passed:results.every(r=>r.passed)&&changed.length===0};
 fs.writeFileSync('artifacts/ui-final-validation.json',JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,suites:results.length,checks:report.checks,geometryRegions:63,changed}));if(!report.passed)process.exitCode=1;
})().catch(error=>{console.error(error);process.exitCode=1});



