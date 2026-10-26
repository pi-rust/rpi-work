// Diagnostic only: exercise the same fixture flow with a click in place of the
// first busy Enter. The required keyboard regression test is unchanged.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
console.log('DIAGNOSTIC: checking whether click succeeds when busy Enter fails');
const file=path.resolve(__dirname,'verify-ui-conversation.cjs');
const source=fs.readFileSync(file,'utf8').replace("await prompt.press('Enter')","await page.getByRole('button',{name:'按所选方式投递消息',exact:true}).click()").replace('busy composer routes Enter and send to selected queue mode without another prompt','diagnostic click path queues both modes; required Enter test is separate').replaceAll('ui-conversation-validation.json','enter-click-diagnostic.json').replaceAll('ui-conversation-failure.png','enter-click-diagnostic.png');
const check=new Module(file,module);check.filename=file;check.paths=Module._nodeModulePaths(__dirname);check._compile(source,file);
