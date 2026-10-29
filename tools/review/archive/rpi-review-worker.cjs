// A local review driver: keep RPC stdin open, discard thinking/snapshot deltas,
// and expose bounded status plus a file command channel in ignored target/.
const fs=require('node:fs');
const path=require('node:path');
const {spawn}=require('node:child_process');
const [name,taskPath]=process.argv.slice(2);
if(!name||!/^[a-z0-9-]+$/.test(name)||!taskPath)throw new Error('Usage: node tools/review/archive/rpi-review-worker.cjs <worker-name> <task.md>');
const root=process.cwd(),dir=path.join(root,'target','parity-workers',name);
fs.mkdirSync(dir,{recursive:true});
const statusPath=path.join(dir,'status.json'),commandPath=path.join(dir,'command.json');
const state={name,pid:null,phase:'starting',startedAt:new Date().toISOString(),updatedAt:null,lastTool:null,toolCalls:0,eventCounts:{},lastResponse:null,outcome:null,finalText:'',stderrLines:0,error:null};
let dirty=true,buffer='',lastCommandId='',timer;
try{lastCommandId=JSON.parse(fs.readFileSync(commandPath,'utf8').replace(/^\uFEFF/,'' )).id||'';}catch{}
function flush(){
  if(!dirty)return;state.updatedAt=new Date().toISOString();
  try{fs.writeFileSync(statusPath+'.tmp',JSON.stringify(state,null,2));fs.renameSync(statusPath+'.tmp',statusPath);dirty=false;}catch{}
}
const child=spawn(process.env.RPI_BIN||'rpi.exe',['--provider','routeryo','--model',process.env.RPI_REVIEW_MODEL||'gpt-6.1-sol','--thinking',process.env.RPI_REVIEW_THINKING||'high','--mode','rpc','--no-session','--no-extensions','--no-skills','--no-prompt-templates','--name',name],{cwd:root,windowsHide:true,env:{...process.env,PYTHONUTF8:'1',PYTHONIOENCODING:'utf-8'},stdio:['pipe','pipe','pipe']});
state.pid=child.pid;
function send(command){child.stdin.write(JSON.stringify(command)+'\n');}
function onEvent(event){
  state.eventCounts[event.type]=(state.eventCounts[event.type]||0)+1;dirty=true;
  if(event.type==='ready'){
    state.phase='ready';
    send({id:`${name}-initial`,type:'prompt',message:fs.readFileSync(path.resolve(root,taskPath),'utf8')});
  }
  if(event.type==='agent_start'){state.phase='working';state.outcome=null;}
  if(event.type==='error')state.error=String(event.message||event.error||'RPI agent error').slice(0,1200);
  if(event.type==='tool_execution_start'){state.toolCalls++;state.lastTool=event.toolName||event.name||'tool';}
  if(event.type==='tool_execution_end'){
    const result=event.result||event.output||{};
    const details=(result.content||[]).filter(item=>item.type==='text').map(item=>item.text||'').join('\n');
    state.recentToolResults??=[];
    state.recentToolResults.push({tool:event.toolName||event.name||state.lastTool,isError:event.isError||result.isError||false,details:details.slice(-2200)});
    state.recentToolResults=state.recentToolResults.slice(-6);
  }
  if(event.type==='response'){
    state.lastResponse={id:event.id,command:event.command,success:event.success};
    if(event.command==='get_state')state.lastResponse.activeOperation=event.data?.activeOperation??null;
    if(!event.success)state.error=String(event.error||'RPC command rejected').slice(0,1200);
  }
  if(event.type==='message_end'&&(event.message?.role==='assistant'||event.message?.kind==='assistant')&&Array.isArray(event.message?.content)){
    if(event.message.errorMessage)state.providerError=String(event.message.errorMessage).slice(0,1000);
    if(event.message.stopReason)state.lastStopReason=event.message.stopReason;
    const text=event.message.content.filter(c=>c.type==='text').map(c=>c.text||'').join('\n');
    if(text)state.finalText=text.slice(-14000);
  }
  if(event.type==='agent_settled'){state.phase='idle';state.outcome=event.outcome??null;flush();}
}
child.stdout.setEncoding('utf8');
child.stdout.on('data',chunk=>{
  buffer+=chunk;
  let end;
  while((end=buffer.indexOf('\n'))>=0){
    const line=buffer.slice(0,end).replace(/\r$/,'');buffer=buffer.slice(end+1);
    try{onEvent(JSON.parse(line));}catch{}
  }
  if(buffer.length>4*1024*1024){state.error='RPC frame exceeded the bounded review buffer';buffer='';dirty=true;}
});
child.stderr.setEncoding('utf8');
child.stderr.on('data',chunk=>{state.stderrLines+=(chunk.match(/\n/g)||[]).length;dirty=true;});
child.on('error',error=>{state.error=error.message;state.phase='failed';dirty=true;flush();});
child.on('close',(code,signal)=>{state.exitCode=code;state.signal=signal;state.phase=code===0?'exited':'failed';dirty=true;flush();clearInterval(timer);});
timer=setInterval(()=>{
  try{
    const command=JSON.parse(fs.readFileSync(commandPath,'utf8').replace(/^\uFEFF/,''));
    if(command.id&&command.id!==lastCommandId){
      if(!['prompt','queue_message','get_state','get_queue','abort','stop'].includes(command.type))throw new Error('Unsupported review command');
      lastCommandId=command.id;send(command);state.lastCommandId=command.id;dirty=true;
    }
  }catch{}
  flush();
},250);
flush();
process.on('SIGINT',()=>send({id:`${name}-shutdown`,type:'stop'}));
