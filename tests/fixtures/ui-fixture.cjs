// Test-only HTTP/SSE node. Never used by the product or real node settings.
const http = require('node:http');
const projects = [{id:'rpi-work',name:'rpi-work',path:'D:/Projects/rpi-work'},{id:'pi-rust',name:'pi-rust',path:'D:/Projects/pi-rust'}];
const seedSessions = [
  {sessionId:'dark',projectId:'rpi-work',title:'为工作台添加深色模式'},
  {sessionId:'architecture',projectId:'rpi-work',title:'梳理 Agent 架构'},
  {sessionId:'mobile',projectId:'rpi-work',title:'检查手机端布局'},
  {sessionId:'rpc',projectId:'pi-rust',title:'验证 RPC 事件协议'},
].map(s=>({...s,status:'idle',createdAt:1792910000000,updatedAt:1792910000000,lastSeq:7}));
const events = [
  {type:'agent_start'},
  {type:'message_update',message:{entryId:'answer'},eventType:'text_delta',delta:'主题切换已经准备好，接下来运行组件测试。'},
  {type:'tool_execution_start',toolCallId:'read-1',toolName:'read',args:{path:'src/theme.rs'}},
  {type:'tool_execution_end',toolCallId:'read-1',toolName:'read',result:{content:[{type:'text',text:'pub fn theme() { /* test fixture */ }'}]}},
  {type:'tool_execution_start',toolCallId:'shell-1',toolName:'bash',args:{command:'cargo test -p work-ui --lib'}},
  {type:'tool_execution_end',toolCallId:'shell-1',toolName:'bash',result:{content:[{type:'text',text:'12 passed; 0 failed (test fixture)'}]}},
  {type:'agent_settled',outcome:'completed',pendingResume:false},
];
async function startFixture(port=0,options={}){
  const requests=[], streams=new Map(), sockets=new Set();
  const fixtureProjects=structuredClone(projects);
  const controls={failPrompt:false,failDialog:false,failQueue:false,uncertainQueue:false,queueDelayMs:0,failProject:false,projectDelayMs:0,failSession:false,sessionDelayMs:0,failRenameCount:0,failCompact:false,compactDelayMs:0};
  const pendingDialogs=new Map(),sessionQueues=new Map();
  let queueCounter=0;
  const sessionEvents=new Map(seedSessions.map(s=>[s.sessionId,events.map((event,i)=>({seq:i+1,event:structuredClone(event)}))]));
  const sessionMessages=new Map(seedSessions.map(s=>[s.sessionId,[{entryId:`user-${s.sessionId}`,role:'user',content:s.sessionId==='dark'?'帮我为工作台增加一个深色主题，并保持手机端可用。':`这是 ${s.title} 的独立历史。`},...(options.includeHistoryAssistant?[{entryId:'answer',role:'assistant',content:[{type:'text',text:'主题切换已经准备好，接下来运行组件测试。'}]}]:[])]]));
  let sessions=structuredClone(seedSessions);
  const sessionConfigs=new Map();
  function sessionConfig(id){if(!sessionConfigs.has(id))sessionConfigs.set(id,{model:'deepseek/deepseek-v4-pro',thinking:'high',activeTools:['read','bash','edit','write']});return sessionConfigs.get(id)}
  const server=http.createServer(async(req,res)=>{
    const origin=req.headers.origin;
    res.setHeader('Access-Control-Allow-Origin',origin || '*');
    res.setHeader('Access-Control-Allow-Credentials','true');
    res.setHeader('Access-Control-Allow-Headers','content-type,x-rpi-web,last-event-id,accept');
    res.setHeader('Access-Control-Allow-Methods','GET,POST,DELETE,OPTIONS');
    if(req.method==='OPTIONS'){res.writeHead(204);res.end();return;}
    const u=new URL(req.url,'http://localhost'), route=u.pathname;
    let body='';for await(const chunk of req)body+=chunk;
    let data={};try{if(body)data=JSON.parse(body);}catch{}
    const authenticated=Boolean(req.headers.cookie?.includes('rpi_web_session=fixture-session-only'));
    requests.push({method:req.method,path:route,query:u.search,body:data,authenticated});
    const json=(value,status=200)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(value));};
    if(route==='/api/v1/health')return json({ok:true,service:'rpi-work-test-fixture',protocol:'rpi-web-v1'});
    if(route==='/api/v1/auth'){
      if(req.method==='POST'){
        if(data.password!=='test-only')return json({error:'Incorrect fixture password'},401);
        res.setHeader('Set-Cookie','rpi_web_session=fixture-session-only; HttpOnly; SameSite=Strict; Path=/');
        // Native rpi-http/rpi-web POST ack is deliberately sparse. Only GET
        // /auth carries `required`; a client must read that authoritative state.
        return json({authenticated:true});
      }
      if(req.method==='DELETE'){res.setHeader('Set-Cookie','rpi_web_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0');return json({authenticated:false});}
      return json({required:Boolean(options.authRequired),authenticated:!options.authRequired||authenticated});
    }
    if(options.authRequired&&!authenticated)return json({error:'Fixture authentication required'},401);
    if(route==='/api/v1/capabilities')return json({protocol:'rpi-web-v1',supportedCommands:['prompt','abort','get_state','get_queue','queue_message','cancel_queued','clear_queue','ui_dialog_response','resume_pending','get_available_models','set_model','get_available_thinking_levels','set_thinking_level','get_available_tools','set_active_tools','get_session_branch','compact',...(options.setApiKeySupported?['set_api_key']:[]),...(options.authStatusSupported?['get_auth_status']:[])],supportedActions:['projects','files','sessions','session_metadata','fork','clone'],parityComplete:false});
    if(route==='/api/v1/projects'){
      if(req.method==='POST'){
        if(controls.projectDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.projectDelayMs));
        if(controls.failProject)return json({error:'Test fixture: project registration rejected'},400);
        const p={id:`project-${fixtureProjects.length}`,name:data.name,path:data.path||`D:/Projects/${data.name}`};fixtureProjects.push(p);return json({project:p});
      }
      return json({projects:fixtureProjects});
    }
    const renameProject=route.match(/^\/api\/v1\/projects\/([^/]+)\/rename$/);
    if(renameProject){const project=fixtureProjects.find(project=>project.id===renameProject[1]);if(controls.failProjectRename)return json({error:'Project rename rejected'},500);if(!project)return json({error:'Not found'},404);project.name=data.name;return json({project});}
    if(route==='/api/v1/project-folders'){
      const current=u.searchParams.get('path')||'';
      if(current.startsWith('/')||/[\\:]/.test(current)||current.split('/').some(part=>part==='..'||part==='.'))return json({error:'invalid project path'},400);
      return json({root:'D:/Projects',path:current,folders:current===''?projects.map(p=>({name:p.name,path:p.path.slice('D:/Projects/'.length)})):[{name:'src',path:`${current.replace(/\/$/,'')}/src`}]});
    }
    if(route==='/api/v1/sessions'){
      if(req.method==='POST'){if(controls.sessionDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.sessionDelayMs));if(controls.failSession)return json({error:'Test fixture: session creation rejected'},500);const s={...seedSessions[0],sessionId:`new-${sessions.length}`,projectId:data.projectId,title:'新会话'};sessions.push(s);return json({sessionId:s.sessionId});}
      return json({sessions});
    }
    if(route.endsWith('/files')){
      const directory=u.searchParams.get('path')||'';
      if(controls.fileDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.fileDelayMs));
      if(controls.failFiles)return json({error:'Test fixture: directory read failed'},500);
      return json({path:directory,entries:controls.fileEntriesByPath?.[directory]||(directory?[]:[{name:'src',path:'src',kind:'directory',size:0},{name:'README.md',path:'README.md',kind:'file',size:60},{name:'preview.html',path:'preview.html',kind:'file',size:100}])});
    }
    if(route.startsWith('/probe-preview'))return json({error:'Preview tried to load a forbidden resource'});
    if(route.endsWith('/file')){
      if(u.searchParams.get('path')==='src/module-2.rs')return json({error:'File deleted in fixture'},404);
      const fixtureOrigin=`http://127.0.0.1:${server.address().port}`;
      if(controls.fileContentByPath?.[u.searchParams.get('path')]!==undefined)return json({path:u.searchParams.get('path'),content:controls.fileContentByPath[u.searchParams.get('path')]});
      return json({path:u.searchParams.get('path'),content:u.searchParams.get('path')?.endsWith('.html')?`<h1>项目预览</h1><p>Test fixture</p><script>parent.__previewExecuted=true</script><img src="${fixtureOrigin}/probe-preview-image"><a href="${fixtureOrigin}/probe-preview-link" target="_self">外部链接</a>`:'# RPI Work\n\nRust / Dioxus 工作台。\n\n```rust\nfn main() {}\n```'});
    }
    if(route.endsWith('/git/status'))return json({isRepository:true,branch:'codex/ui-parity',repositoryRoot:'D:/Projects/rpi-work',worktreePath:'D:/Projects/rpi-work',files:Array.from({length:120},(_,i)=>({path:i?`src/module-${i}.rs`:'src/theme.rs',originalPath:null,status:i===2?'D':'M',staged:false,unstaged:true}))});
    if(route.endsWith('/git/diff'))return json({path:u.searchParams.get('path'),patch:'diff --git a/src/theme.rs b/src/theme.rs\n--- a/src/theme.rs\n+++ b/src/theme.rs\n@@ -1 +1 @@\n-let theme = "light";\n+let theme = "dark";'});
    const match=route.match(/^\/api\/v1\/sessions\/([^/]+)(?:\/(.*))?$/);
    if(match){
      const [,id,action]=match,s=sessions.find(s=>s.sessionId===id)||seedSessions[0];
      if(!action)return json(s);
      if(action==='open')return json(s);
      if(action==='snapshot'){
        const history=sessionEvents.get(id)||[];
        return json({sessionId:id,messages:sessionMessages.get(id)||[],lastSeq:history.at(-1)?.seq||0,events:history,pendingDialogs:pendingDialogs.get(id)||[],latestSettlement:history.findLast(e=>e.event.type==='agent_settled')?.event});
      }
      if(action==='events'){
        res.writeHead(200,{'content-type':'text/event-stream','cache-control':'no-cache','connection':'keep-alive'});
        res.write(': test fixture connected\n\n');streams.set(res,id);res.on('close',()=>streams.delete(res));return;
      }
      if(action==='rename'){if(controls.failRenameCount>0){controls.failRenameCount--;return json({error:'Test fixture: rename temporarily failed'},500);}s.title=data.title;return json({sessionId:id,title:s.title,status:s.status});}
      if(action==='metadata'){if(controls.failMetadata)return json({error:'Metadata rejected'},500);if(typeof data.archived==='boolean')s.archived=data.archived;if(typeof data.pinned==='boolean')s.pinned=data.pinned;return json({sessionId:id,archived:!!s.archived,pinned:!!s.pinned});}
      if(action==='fork'||action==='clone'){if(controls.failFork)return json({error:'Fork rejected'},500);const nextId=`branch-${sessions.length}`;let history=structuredClone(sessionMessages.get(id)||[]);if(data.entryId){const cut=history.findIndex(message=>message.entryId===data.entryId);if(cut<0)return json({error:'Unknown entry'},400);history=history.slice(0,cut+1);}const branch={...structuredClone(s),sessionId:nextId,title:data.title||'分支',archived:false,pinned:false,status:'idle'};sessions.push(branch);sessionMessages.set(nextId,history);sessionEvents.set(nextId,[]);return json({sessionId:nextId,sourceSessionId:id,title:branch.title,status:'idle'});}
      if(action==='commands'){
        const command=data.type,config=sessionConfig(id);
        let queue=sessionQueues.get(id)||[];
        const response=value=>json({type:'response',id:data.id??null,command,success:true,data:value,error:null});
        if(command==='prompt'&&controls.promptDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.promptDelayMs));
        if(command==='prompt'&&controls.failPrompt)return json({type:'response',id:data.id??null,command,success:false,data:null,error:'Test fixture: prompt rejected'});
        const runtime=()=>({modelId:config.model,provider:config.model.split('/')[0],thinkingLevel:config.thinking,...(controls.omitActiveTools?{}:{activeTools:config.activeTools}),context:controls.runtimeContext===undefined?{tokens:38000,window:100000,threshold:84000,percent:38,estimated:false}:controls.runtimeContext,stats:controls.runtimeStats===undefined?{cachedTokens:24000,uncachedTokens:8000,totalTokens:36800,costTotal:0.0184,cacheHitRatio:0.75}:controls.runtimeStats,queue:{count:queue.length}});
        if(command==='get_state')return response(runtime());
        if(command==='get_session_branch'){
          const entries=structuredClone(controls.branchEntries||[]),failure=controls.branchError||(controls.failBranch?'unknown command type: get_session_branch':null);
          if(controls.branchDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.branchDelayMs));
          if(failure)return json({type:'response',id:data.id??null,command,success:false,error:failure});
          return response({entries,count:entries.length});
        }
        if(command==='compact'){
          if(controls.compactDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.compactDelayMs));
          if(controls.failCompact)return json({type:'response',command,success:false,error:'Test fixture: compaction rejected'});
          return response(controls.compactResult||{accepted:true});
        }
        if(command==='get_queue')return response({items:queue,count:queue.length});
        if(command==='get_available_models')return response({models:controls.models||[{id:'deepseek/deepseek-v4-pro',name:'DeepSeek',provider:'deepseek',reasoning:true,contextWindow:100000},{id:'openai/gpt-6-astra',name:'OpenAI',provider:'openai',reasoning:true}]});
        if(command==='get_auth_status'){
          if(controls.failAuthStatus)return json({type:'response',command,success:false,error:'Test fixture: provider status unavailable'});
          return response({providers:controls.authStatuses||[{provider:'deepseek',ready:true,source:'environment',status:'ready',oauth:false,oauthAvailable:false},{provider:'openai',ready:false,source:'missing',status:'missing',oauth:false,oauthAvailable:false}]});
        }
        if(command==='get_available_thinking_levels')return response({levels:['off','low','medium','high']});
        if(command==='get_available_tools')return response({tools:['read','bash','edit','write','ask_user','todo'],activeTools:config.activeTools});
        if(command==='set_model'){config.model=data.modelId;return response(runtime());}
        if(command==='set_thinking_level'){config.thinking=data.level;return response(runtime());}
        if(command==='set_active_tools'){if(controls.toolsDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.toolsDelayMs));if(controls.failSetTools)return json({type:'response',id:data.id??null,command,success:false,error:'Test fixture: tool configuration rejected'});if(!controls.ignoreSetToolsMutation)config.activeTools=[...data.tools];return response(runtime());}
        if(command==='set_api_key'&&options.setApiKeySupported){
          if(controls.keyDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.keyDelayMs));
          if(controls.failSetKey)return json({type:'response',command,success:false,error:'Test fixture: key rejected'});
          return response({provider:data.provider,ready:true,source:'test-only fixture',requiresRestart:true});
        }
        if(command==='queue_message'){
          if(controls.queueDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.queueDelayMs));
          if(controls.failQueue)return json({type:'response',command,success:false,error:'Test fixture: queue rejected'});
          const item={entryId:`q-${queueCounter++}`,queue:data.deliverAs||'followUp',text:data.message,status:'queued'};queue.push(item);sessionQueues.set(id,queue);return response({...(controls.uncertainQueue?{}:{accepted:true}),entryId:item.entryId,queue:item.queue,status:'queued',outcome:'queued'});
        }
        if(command==='cancel_queued'){const found=queue.find(item=>item.entryId===data.entryId);sessionQueues.set(id,queue.filter(item=>item.entryId!==data.entryId));return response({entryId:data.entryId,cancelled:Boolean(found),outcome:found?'cancelled':'not_found'});}
        if(command==='clear_queue'){sessionQueues.set(id,[]);return response({count:0});}
        if(command==='ui_dialog_response'){
          if(controls.failDialog)return json({type:'response',command,success:false,error:'Test fixture: dialog response rejected'});
          pendingDialogs.set(id,(pendingDialogs.get(id)||[]).filter(p=>p.requestId!==data.requestId));return response({accepted:true,requestId:data.requestId});
        }
        return response({accepted:true});
      }
    }
    return json({error:`Unknown fixture route: ${req.method} ${route}`},404);
  });
  server.on('connection',s=>{sockets.add(s);s.on('close',()=>sockets.delete(s));});
  await new Promise(resolve=>server.listen(port,'127.0.0.1',resolve));
  return {
    url:`http://127.0.0.1:${server.address().port}/`,requests,controls,
    setQueue(items,sessionId='dark'){sessionQueues.set(sessionId,structuredClone(items));},
    emit(event,sessionId='dark'){
      const history=sessionEvents.get(sessionId)||[],seq=(history.at(-1)?.seq||0)+1;
      history.push({seq,event:structuredClone(event)});sessionEvents.set(sessionId,history);
      const session=sessions.find(item=>item.sessionId===sessionId);
      if(session){session.lastSeq=seq;if(event.type==='agent_start')session.status='running';if(event.type==='agent_settled'){session.status=event.outcome==='completed'?'idle':event.outcome;session.pendingResume=Boolean(event.pendingResume);session.latestSettlement=structuredClone(event);}}
      if(event.type==='ui_dialog_request'){
        const current=pendingDialogs.get(sessionId)||[];
        if(!current.some(p=>p.requestId===event.requestId))current.push({requestId:event.requestId,toolCallId:event.toolCallId||null,ui:event.ui||event});
        pendingDialogs.set(sessionId,current);
      }
      if(event.type==='ui_dialog_closed')pendingDialogs.set(sessionId,(pendingDialogs.get(sessionId)||[]).filter(p=>p.requestId!==event.requestId));
      if(event.type==='message_end'&&event.message){
        const messages=sessionMessages.get(sessionId)||[],key=event.message.entryId||event.message.messageId;
        const existing=key?messages.findIndex(m=>(m.entryId||m.messageId)===key):-1;
        if(existing>=0)messages[existing]=structuredClone(event.message);else messages.push(structuredClone(event.message));
        sessionMessages.set(sessionId,messages);
      }
      for(const [res,id] of streams)if(id===sessionId)res.write(`id: ${seq}\nevent: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
      return seq;
    },
    disconnect(sessionId='dark',error=false){for(const [res,id] of streams)if(id===sessionId){if(error)res.destroy();else res.end();}},
    async close(){for(const s of sockets)s.destroy();await new Promise(resolve=>server.close(resolve));}
  };
}
module.exports={startFixture};

