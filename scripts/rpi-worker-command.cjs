const fs=require('node:fs'),path=require('node:path');
const [name,action,file]=process.argv.slice(2);
if(!name||!/^[a-z0-9-]+$/.test(name))throw new Error('Invalid worker name');
const command={id:`${name}-${Date.now()}`};
if(action==='status')command.type='get_state';
else if(action==='stop')command.type='stop';
else if(action==='abort')command.type='abort';
else if(action==='prompt'||action==='steer'){
  command.type=action==='steer'?'queue_message':'prompt';
  if(action==='steer')command.deliverAs='steer';
  command.message=fs.readFileSync(path.resolve(file),'utf8');
}else throw new Error('Use status, stop, abort, prompt <task.md>, or steer <feedback.md>');
const dir=path.join('target','parity-workers',name);
if(!fs.existsSync(path.join(dir,'status.json')))throw new Error('Worker does not exist');
// A steer queued after settlement does not start another native run. Deliver it
// as a new prompt when the worker is already idle, avoiding silent stale work.
if(action==='steer'){
  const state=JSON.parse(fs.readFileSync(path.join(dir,'status.json'),'utf8'));
  if(state.phase==='idle'){command.type='prompt';delete command.deliverAs;}
}
const target=path.join(dir,'command.json');
fs.writeFileSync(target+'.tmp',JSON.stringify(command));fs.renameSync(target+'.tmp',target);
console.log(`Sent ${command.type} to ${name}`);
