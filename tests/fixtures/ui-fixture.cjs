// Test-only HTTP/WebSocket node. Never used by the product or real node settings.
const http = require('node:http');
const crypto = require('node:crypto');
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
// Normalize logical actions for assertions without changing recorded wire bodies.
function fixtureCommand(request) {
  return request.method === 'POST' && /\/sessions\/[^/]+\/dialogs$/.test(request.path)
    ? 'ui_dialog_response' : request.body.type;
}
const WS_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
function websocketFrame(text, opcode = 1) {
  const payload = Buffer.from(text);
  const header = [0x80 | opcode];
  if (payload.length < 126) header.push(payload.length);
  else if (payload.length <= 0xffff) header.push(126, payload.length >> 8, payload.length & 0xff);
  else throw new Error('fixture websocket frame too large');
  return Buffer.concat([Buffer.from(header), payload]);
}
function websocketAccept(key) {
  return crypto.createHash('sha1').update(key + WS_GUID).digest('base64');
}
function readWebSocketFrames(connection, chunk, onText) {
  connection.buffer = Buffer.concat([connection.buffer, chunk]);
  while (connection.buffer.length >= 2) {
    const first = connection.buffer[0], second = connection.buffer[1];
    const opcode = first & 0x0f, masked = Boolean(second & 0x80);
    let length = second & 0x7f, offset = 2;
    if (length === 126) { if (connection.buffer.length < 4) return; length = connection.buffer.readUInt16BE(2); offset = 4; }
    else if (length === 127) throw new Error('fixture websocket frame length overflow');
    if (!masked || connection.buffer.length < offset + 4 + length) return;
    const mask = connection.buffer.subarray(offset, offset + 4); offset += 4;
    const payload = Buffer.from(connection.buffer.subarray(offset, offset + length));
    connection.buffer = connection.buffer.subarray(offset + length);
    for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i % 4];
    if (opcode === 1) onText(payload.toString('utf8'));
    else if (opcode === 8) { connection.socket.write(websocketFrame('', 8)); connection.socket.end(); return; }
    else if (opcode === 9) connection.socket.write(websocketFrame(payload.toString('utf8'), 10));
  }
}
async function startFixture(port=0,options={}){
  const requests=[], sockets=new Set(), wsConnections=new Set();
  const fixtureProjects=structuredClone(projects);
  const controls={failPrompt:false,failDialog:false,failQueue:false,uncertainQueue:false,queueDelayMs:0,cancelDelayMs:0,failCancel:false,cancelOutcome:null,failProject:false,projectDelayMs:0,failSession:false,sessionDelayMs:0,failRenameCount:0,failCompact:false,compactDelayMs:0};
  const defaultCommands=['prompt','abort','get_state',...(options.pagedHistory&&!options.noMessagePaging?['get_messages']:[]),...(options.nativeQueries?['get_message_summaries','get_message_window']:[]),'get_queue','queue_message','cancel_queued','clear_queue','ui_dialog_response','resume_pending','get_available_models','set_model','get_available_thinking_levels','set_thinking_level','get_available_tools','set_active_tools','get_session_branch','compact',...(options.setApiKeySupported?['set_api_key']:[]),...(options.authStatusSupported?['get_auth_status']:[])];
  // These restrictions are session observations, never global capability fallbacks.
  function sessionAccess(id) {
    const restriction=controls.sessionAccessById?.[id]??controls.sessionAccess??options.sessionAccessById?.[id]??options.sessionAccess??{};
    const supportedCommands=defaultCommands.filter(command=>!restriction.supportedCommands||restriction.supportedCommands.includes(command));
    const dialogReply=supportedCommands.includes('ui_dialog_response')&&restriction.dialogReply!==false;
    const capabilities={sessions:true,sessionLoad:true,sessionResume:false,prompt:supportedCommands.includes('prompt'),streaming:true,abort:supportedCommands.includes('abort'),dialogs:dialogReply,projects:true,files:true,gitDiff:true,models:supportedCommands.includes('get_available_models'),thinkingLevels:supportedCommands.includes('get_available_thinking_levels'),tools:supportedCommands.includes('get_available_tools'),providerManagement:supportedCommands.includes('set_api_key'),queue:supportedCommands.includes('get_queue'),history:supportedCommands.includes('get_session_branch')};
    for(const key of Object.keys(capabilities))if(restriction.capabilities?.[key]===false)capabilities[key]=false;
    return {descriptor:{identity:{protocol:'rpi',version:null,transport:'websocket',agentName:'ui-fixture'},capabilities},supportedCommands,snapshotLimit:restriction.snapshotLimit!==false,dialogReply:dialogReply&&capabilities.dialogs,...((controls.promptContent??options.promptContent)?{promptContent:controls.promptContent??options.promptContent}:{})};
  }
  function replyDialog(id,data) {
    if(controls.failDialog)return 'Test fixture: dialog response rejected';
    pendingDialogs.set(id,(pendingDialogs.get(id)||[]).filter(p=>p.requestId!==data.requestId));
    return null;
  }
  const pendingDialogs=new Map(),sessionQueues=new Map();
  let queueCounter=0;
  const sessionEvents=new Map(seedSessions.map(s=>[s.sessionId,events.map((event,i)=>({seq:i+1,event:structuredClone(event)}))]));
  const sessionMessages=new Map(seedSessions.map(s=>[s.sessionId,[{entryId:`user-${s.sessionId}`,role:'user',content:s.sessionId==='dark'?'帮我为工作台增加一个深色主题，并保持手机端可用。':`这是 ${s.title} 的独立历史。`},...(options.includeHistoryAssistant?[{entryId:'answer',role:'assistant',content:[{type:'text',text:'主题切换已经准备好，接下来运行组件测试。'}]}]:[])]]));
  let sessions=structuredClone(seedSessions);
  // Backend catalog/bindings are opt-in; existing fixtures remain RPI-only.
  const sessionBackends=new Map(seedSessions.map(s=>[s.sessionId,options.sessionBackends?.[s.sessionId]||'rpi']));
  const sessionConfigs=new Map();
  function sessionConfig(id){if(!sessionConfigs.has(id))sessionConfigs.set(id,{model:'deepseek/deepseek-v4-pro',thinking:'high',activeTools:['read','bash','edit','write']});return sessionConfigs.get(id)}
  // Native-query controls are opt-in; legacy fixtures retain their exact wire shape.
  // leafId/leafIdById override the selected branch leaf. Each endpoint supports
  // DelayMs, Status (404/501/409/etc), and optional FailCount for one-shot errors.
  function selectedLeaf(id) {
    return controls.leafIdById?.[id] ?? controls.leafId ?? sessionMessages.get(id)?.at(-1)?.entryId ?? null;
  }
  function selectedMessages(id) {
    const all = sessionMessages.get(id) || [];
    if (!options.nativeQueries) return all;
    const leaf = selectedLeaf(id), end = all.findIndex(message => message.entryId === leaf);
    return end < 0 ? [] : all.slice(0, end + 1);
  }
  function messagePage(id, limit, beforeId = null) {
    const all = selectedMessages(id);
    const end = beforeId === null ? all.length : all.findIndex(m => m.entryId === beforeId);
    if (end < 0) return null;
    const start = Math.max(0, end - Math.max(1, Math.min(400, Number(limit) || 48)));
    const messages = structuredClone(all.slice(start, end));
    return { messages, total: all.length, firstId: messages[0]?.entryId || null, lastId: messages.at(-1)?.entryId || null, hasMore: start > 0, truncated: start > 0, beforeId };
  }
  function wsSend(connection, frame) {
    if (!connection.socket.destroyed) connection.socket.write(websocketFrame(JSON.stringify(frame)));
  }
  function wireEvent(entry) {
    return {id:String(entry.seq),seq:entry.seq,eventType:entry.event.type,data:structuredClone(entry.event)};
  }
  function hostResponse(command, data = {accepted:true}, success = true, error = 'Test fixture: command rejected') {
    return {type:'response',id:null,command,success,data,error:success?null:error};
  }
  function runtimeState(id) {
    const config=sessionConfig(id);
    const queue=sessionQueues.get(id)||[];
    return {modelId:config.model,provider:config.model.split('/')[0],thinkingLevel:config.thinking,...(controls.omitActiveTools?{}:{activeTools:config.activeTools}),context:controls.runtimeContext===undefined?{tokens:38000,window:100000,threshold:84000,percent:38,estimated:false}:controls.runtimeContext,stats:controls.runtimeStats===undefined?{cachedTokens:24000,uncachedTokens:8000,totalTokens:36800,costTotal:0.0184,cacheHitRatio:0.75}:controls.runtimeStats,queue:{count:queue.length}};
  }
  async function websocketCommand(id, payload) {
    const command=payload?.type, config=sessionConfig(id), queue=sessionQueues.get(id)||[];
    if(!sessionAccess(id).supportedCommands.includes(command)) return hostResponse(command,null,false);
    if(command==='get_messages' && options.pagedHistory) {
      if(controls.historyDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.historyDelayMs));
      if(controls.failHistoryCount>0) { controls.failHistoryCount--; return hostResponse(command,null,false); }
      const page=messagePage(id,payload.limit,controls.ignoreHistoryCursor?null:(payload.beforeId||null));
      return page?hostResponse(command,page):hostResponse(command,null,false);
    }
    if(command==='prompt'&&controls.promptDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.promptDelayMs));
    if(command==='prompt'&&controls.failPrompt) return hostResponse(command,null,false);
    if(command==='get_state') return hostResponse(command,runtimeState(id));
    if(command==='get_session_branch') {
      const entries=structuredClone(controls.branchEntries||[]), failure=controls.branchError||(controls.failBranch?'unknown command type: get_session_branch':null);
      if(controls.branchDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.branchDelayMs));
      return failure?hostResponse(command,null,false):hostResponse(command,{entries,count:entries.length});
    }
    if(command==='compact') {
      if(controls.compactDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.compactDelayMs));
      return controls.failCompact?hostResponse(command,null,false):hostResponse(command,controls.compactResult||{accepted:true});
    }
    if(command==='get_queue') return hostResponse(command,{items:queue,count:queue.length});
    if(command==='get_available_models') return hostResponse(command,{models:controls.models||[{id:'deepseek/deepseek-v4-pro',name:'DeepSeek',provider:'deepseek',reasoning:true,contextWindow:100000},{id:'openai/gpt-6-astra',name:'OpenAI',provider:'openai',reasoning:true}]});
    if(command==='get_auth_status') return controls.failAuthStatus?hostResponse(command,null,false):hostResponse(command,{providers:controls.authStatuses||[{provider:'deepseek',ready:true,source:'environment',status:'ready',oauth:false,oauthAvailable:false},{provider:'openai',ready:false,source:'missing',status:'missing',oauth:false,oauthAvailable:false}]});
    if(command==='get_available_thinking_levels') return hostResponse(command,{levels:['off','low','medium','high']});
    if(command==='get_available_tools') return hostResponse(command,{tools:['read','bash','edit','write','ask_user','todo'],activeTools:config.activeTools});
    if(command==='set_model') { config.model=payload.modelId; return hostResponse(command,runtimeState(id)); }
    if(command==='set_thinking_level') { config.thinking=payload.level; return hostResponse(command,runtimeState(id)); }
    if(command==='set_active_tools') {
      if(controls.toolsDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.toolsDelayMs));
      if(controls.failSetTools) return hostResponse(command,null,false);
      if(!controls.ignoreSetToolsMutation) config.activeTools=[...(payload.tools||[])];
      return hostResponse(command,runtimeState(id));
    }
    if(command==='set_api_key'&&options.setApiKeySupported) return controls.failSetKey?hostResponse(command,null,false):hostResponse(command,{provider:payload.provider,ready:true,source:'test-only fixture',requiresRestart:true});
    if(command==='queue_message') {
      if(controls.queueDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.queueDelayMs));
      if(controls.failQueue) return hostResponse(command,null,false);
      const item={entryId:`q-${queueCounter++}`,queue:payload.deliverAs||'followUp',text:payload.message,status:'queued'}; queue.push(item); sessionQueues.set(id,queue);
      return hostResponse(command,{...(controls.uncertainQueue?{}:{accepted:true}),entryId:item.entryId,queue:item.queue,status:'queued',outcome:'queued'});
    }
    if(command==='cancel_queued') {
      if(controls.cancelDelayMs) await new Promise(resolve=>setTimeout(resolve,controls.cancelDelayMs));
      if(controls.failCancel) return hostResponse(command,null,false);
      const found=queue.find(item=>item.entryId===payload.entryId), outcome=controls.cancelOutcome||(found?'cancelled':'not_found');
      if(outcome==='cancelled'||outcome==='canceled') sessionQueues.set(id,queue.filter(item=>item.entryId!==payload.entryId));
      return hostResponse(command,{entryId:payload.entryId,cancelled:outcome==='cancelled'||outcome==='canceled',outcome});
    }
    if(command==='clear_queue') { sessionQueues.set(id,[]); return hostResponse(command,{count:0}); }
    if(command==='ui_dialog_response') { const error=replyDialog(id,payload); return error?hostResponse(command,null,false):hostResponse(command,{accepted:true,requestId:payload.requestId}); }
    return hostResponse(command,{accepted:true});
  }
  function installWebSocket(req, socket) {
    const authenticated=Boolean(req.headers.cookie?.includes('rpi_web_session=fixture-session-only'));
    if(options.authRequired && !authenticated) { socket.write('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); socket.destroy(); return; }
    const key=req.headers['sec-websocket-key'];
    if(!key) { socket.destroy(); return; }
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Protocol: rpi-work.realtime.v1\r\nSec-WebSocket-Accept: ${websocketAccept(key)}\r\n\r\n`);
    const connection={socket,buffer:Buffer.alloc(0),subscriptions:new Map(),operations:new Map()};
    wsConnections.add(connection); sockets.add(socket);
    const connectionId=`fixture-connection-${wsConnections.size}`;
    wsSend(connection,{type:'hello',version:1,hello:{version:1,nodeId:'fixture-node',ownerEpoch:'fixture-owner',connectionId,capabilities:{jsonText:true,binary:false,commandQueries:true,maxMessageBytes:2097152,maxEventsPerFrame:256}}});
    socket.on('data',chunk=>readWebSocketFrames(connection,chunk,async text=>{
      let frame; try { frame=JSON.parse(text); } catch { return; }
      const type=frame.type, requestId=frame.requestId;
      if(type==='ping') return wsSend(connection,{type:'pong',version:1,requestId});
      if(type==='reserve') {
        const operation={ownerEpoch:'fixture-owner',operationId:`fixture-operation-${connection.operations.size+1}`};
        connection.operations.set(operation.operationId,{session:frame.sessionId});
        return wsSend(connection,{type:'reserved',version:1,requestId,operation});
      }
      if(type==='execute') {
        const operation=frame.operation, entry=connection.operations.get(operation?.operationId);
        const value=await websocketCommand(entry?.session||'dark',frame.payload||{});
        return wsSend(connection,{type:'status',version:1,requestId,operation,status:{state:'knownResult',outcome:{status:'succeeded',value}}});
      }
      if(type==='query') {
        const value=await websocketCommand(frame.sessionId,frame.payload||{});
        return wsSend(connection,{type:'result',version:1,requestId,value});
      }
      if(type==='subscribe') {
        const id=frame.sessionId, after=Number(frame.afterSeq||0), subscription=frame.subscriptionId;
        requests.push({method:'GET',path:'/api/v1/realtime',query:`?after=${after}`,sessionId:id,subscriptionId:subscription,body:{},authenticated:true});
        connection.subscriptions.set(subscription,id);
        wsSend(connection,{type:'subscribed',version:1,requestId,sessionId:id,subscriptionId:subscription});
        wsSend(connection,{type:'events',version:1,sessionId:id,subscriptionId:subscription,events:[{id:null,seq:null,eventType:'ready',data:{type:'ready',sessionId:id}}]});
        for(const entry of sessionEvents.get(id)||[]) if(entry.seq>after) wsSend(connection,{type:'events',version:1,sessionId:id,subscriptionId:subscription,events:[wireEvent(entry)]});
        return;
      }
      if(type==='unsubscribe') { connection.subscriptions.delete(frame.subscriptionId); return wsSend(connection,{type:'unsubscribed',version:1,requestId,subscriptionId:frame.subscriptionId}); }
    }));
    socket.on('close',()=>{wsConnections.delete(connection);sockets.delete(socket);});
  }
  const server=http.createServer(async(req,res)=>{
    const origin=req.headers.origin;
    res.setHeader('Access-Control-Allow-Origin',origin || '*');
    res.setHeader('Access-Control-Allow-Credentials','true');
    res.setHeader('Access-Control-Allow-Headers','content-type,x-rpi-web,accept');
    res.setHeader('Access-Control-Allow-Methods','GET,POST,DELETE,OPTIONS');
    if(req.method==='OPTIONS'){res.writeHead(204);res.end();return;}
    const u=new URL(req.url,'http://localhost'), route=u.pathname;
    let body='';for await(const chunk of req)body+=chunk;
    let data={};try{if(body)data=JSON.parse(body);}catch{}
    const authenticated=Boolean(req.headers.cookie?.includes('rpi_web_session=fixture-session-only'));
    requests.push({method:req.method,path:route,query:u.search,body:data,authenticated});
    const json=(value,status=200)=>{res.writeHead(status,{'content-type':'application/json'});res.end(JSON.stringify(value));};
    if(route==='/api/v1/health')return json({ok:true,service:'rpi-work-test-fixture',protocol:'rpi-web-v1',nodeId:'fixture-node'});
    if(route==='/api/v1/transports')return json({version:1,nodeId:'fixture-node',ownerEpoch:'fixture-owner',path:'/api/v1/realtime',subprotocol:'rpi-work.realtime.v1',capabilities:{jsonText:true,binary:false,commandQueries:true,maxMessageBytes:2097152,maxEventsPerFrame:256}});
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
    if(route==='/api/v1/capabilities')return json({protocol:'rpi-web-v1',supportedCommands:defaultCommands,supportedActions:['projects','files','sessions','session_metadata','fork','clone'],parityComplete:false});
    if(route==='/api/v1/agent-backends')return json({backends:options.agentBackends||[{id:'rpi',name:'RPI fixture',available:true,reason:null}]});
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
      if(req.method==='POST'){if(controls.sessionDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.sessionDelayMs));if(controls.failSession)return json({error:'Test fixture: session creation rejected'},500);const s={...seedSessions[0],sessionId:`new-${sessions.length}`,projectId:data.projectId,title:'新会话'};sessions.push(s);sessionBackends.set(s.sessionId,options.agentBackends?(data.backend||'rpi'):'rpi');return json({sessionId:s.sessionId});}
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
      if(action==='agent-binding'){
        // Opt-in faults let browser tests observe unknown binding without ACP.
        if(options.agentBackends&&controls.bindingDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.bindingDelayMs));
        if(options.agentBackends&&controls.failBinding)return json({error:'Test fixture: binding unavailable'},503);
        return json({backend:sessionBackends.get(id)||'rpi',externalSessionId:null});
      }
      if(action==='snapshot'){
        const history=sessionEvents.get(id)||[];
        const page = options.pagedHistory || options.nativeQueries ? messagePage(id, u.searchParams.get('limit')) : { messages:sessionMessages.get(id)||[] };
        return json({sessionId:id,...page,...(options.nativeQueries?{leafId:selectedLeaf(id)}:{}),lastSeq:history.at(-1)?.seq||0,events:history,pendingDialogs:pendingDialogs.get(id)||[],latestSettlement:history.findLast(e=>e.event.type==='agent_settled')?.event});
      }
      if(options.nativeQueries&&(action==='message-summaries'||action==='message-window')){
        if(req.method!=='GET')return json({error:'Unsupported fixture method'},405);
        if(!sessions.some(session=>session.sessionId===id))return json({error:'Unknown fixture session'},404);
        const prefix=action==='message-summaries'?'messageSummaries':'messageWindow';
        const delay=controls[`${prefix}DelayMs`]??controls.nativeQueryDelayMs??0;
        // Capture both the branch and fault before waiting, so delayed replies
        // faithfully test a late response after controls/session selection change.
        const leafId=selectedLeaf(id),all=structuredClone(selectedMessages(id));
        const countKey=`${prefix}FailCount`,status=controls[`${prefix}Status`]??controls.nativeQueryStatus;
        const fail=status&&(controls[countKey]===undefined||controls[countKey]>0);
        if(fail&&controls[countKey]>0)controls[countKey]--;
        if(delay)await new Promise(resolve=>setTimeout(resolve,delay));
        if(fail)return json({error:`Test fixture: ${action} unavailable (${status})`},status);
        const pin=u.searchParams.get('leafId');
        if(pin&&pin!==leafId)return json({error:'Test fixture: selected leaf changed'},409);
        const limit=Number(u.searchParams.get('limit')||(action==='message-summaries'?10:48));
        if(!Number.isInteger(limit)||limit<1||limit>100)return json({error:'Invalid native query limit'},400);
        const before=u.searchParams.get('beforeId'),after=u.searchParams.get('afterId'),anchor=u.searchParams.get('anchorId');
        const index=cursor=>all.findIndex(message=>message.entryId===cursor);
        if([before,after,anchor].some(cursor=>cursor!==null&&index(cursor)<0))return json({error:'Test fixture: cursor is not on selected leaf'},409);
        if(action==='message-summaries'){
          if((u.searchParams.get('role')||'user')!=='user'||anchor||after)return json({error:'Invalid summary query'},400);
          const users=all.slice(0,before?index(before):all.length).filter(message=>message.role==='user');
          const messages=users.slice(-limit).map(message=>{
            const text=typeof message.content==='string'?message.content:(message.content||[]).filter(block=>block.type==='text').map(block=>block.text||'').join(' ');
            const preview=Array.from(text.replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2060-\u2069]/gu,' ').replace(/\s+/gu,' ').trim()).slice(0,140).join('');
            return {entryId:message.entryId,role:'user',preview,timestamp:0,hasImages:false};
          });
          const firstId=messages[0]?.entryId||null,lastId=messages.at(-1)?.entryId||null,hasMore=users.length>messages.length;
          return json({filtered:true,role:'user',messages,firstId,lastId,nextBeforeId:hasMore?firstId:null,hasMore,leafId});
        }
        if([before,after,anchor].filter(cursor=>cursor!==null).length!==1)return json({error:'Exactly one window cursor required'},400);
        let start,end;
        if(before){end=index(before);start=Math.max(0,end-limit);}
        else if(after){start=index(after)+1;end=Math.min(all.length,start+limit);}
        else {start=Math.max(0,index(anchor)-Math.floor(limit/2));end=Math.min(all.length,start+limit);start=Math.max(0,end-limit);}
        const messages=all.slice(start,end),firstId=messages[0]?.entryId||null,lastId=messages.at(-1)?.entryId||null;
        const hasMoreBefore=start>0,hasMoreAfter=end<all.length;
        return json({messages,firstId,lastId,nextBeforeId:hasMoreBefore?firstId:null,nextAfterId:hasMoreAfter?lastId:null,hasMoreBefore,hasMoreAfter,leafId});
      }
      if(action==='rename'){if(controls.failRenameCount>0){controls.failRenameCount--;return json({error:'Test fixture: rename temporarily failed'},500);}s.title=data.title;return json({sessionId:id,title:s.title,status:s.status});}
      if(action==='metadata'){if(controls.failMetadata)return json({error:'Metadata rejected'},500);if(typeof data.archived==='boolean')s.archived=data.archived;if(typeof data.pinned==='boolean')s.pinned=data.pinned;return json({sessionId:id,archived:!!s.archived,pinned:!!s.pinned});}
      if(action==='fork'||action==='clone'){if(controls.failFork)return json({error:'Fork rejected'},500);const nextId=`branch-${sessions.length}`;let history=structuredClone(sessionMessages.get(id)||[]);if(data.entryId){const cut=history.findIndex(message=>message.entryId===data.entryId);if(cut<0)return json({error:'Unknown entry'},400);history=history.slice(0,cut+1);}const branch={...structuredClone(s),sessionId:nextId,title:data.title||'分支',archived:false,pinned:false,status:'idle'};sessions.push(branch);sessionMessages.set(nextId,history);sessionEvents.set(nextId,[]);return json({sessionId:nextId,sourceSessionId:id,title:branch.title,status:'idle'});}
      if(action==='access'||action==='dialogs'){
        res.setHeader('Cache-Control','no-store');
        if(!sessions.some(session=>session.sessionId===id))return json({error:'Unknown fixture session'},404);
        if(action==='access'){
          if(req.method!=='GET')return json({error:'Unsupported fixture method'},405);
          if(controls.accessStatus)return json({error:'Test fixture: session access unavailable'},controls.accessStatus);
          return json(sessionAccess(id));
        }
        if(req.method!=='POST')return json({error:'Unsupported fixture method'},405);
        if(!data||typeof data!=='object'||Array.isArray(data)||typeof data.requestId!=='string'||!data.requestId||Object.keys(data).some(key=>!['requestId','answer','cancelled'].includes(key))||!(Object.hasOwn(data,'answer')&&!Object.hasOwn(data,'cancelled')||!Object.hasOwn(data,'answer')&&data.cancelled===true))return json({error:'Invalid fixture dialog reply'},400);
        if(!sessionAccess(id).dialogReply)return json({error:'Test fixture: dialog reply unsupported'},501);
        const error=replyDialog(id,data);
        return error?json({error},400):json({accepted:true,requestId:data.requestId});
      }
      if(action==='configuration'){
        if(!sessions.some(session=>session.sessionId===id))return json({error:'Unknown fixture session'},404);
        res.setHeader('Cache-Control','no-store');
        if(req.method==='GET')return json({protocol:'rpi',supportedCommands:controls.configurationCommands||['get_available_models','set_model','get_available_thinking_levels','set_thinking_level','get_state','get_available_tools','set_active_tools','get_session_branch','compact'],configOptions:[]});
        if(req.method!=='POST')return json({error:'Unsupported fixture method'},405);
        if(!Object.hasOwn(data,'value')||typeof data.configId!=='string'||typeof data.value!=='string')return json({error:'Invalid fixture configuration'},400);
        const config=sessionConfig(id);
        if(data.configId==='model')config.model=data.value;
        else if(data.configId==='thinking')config.thinking=data.value;
        else return json({error:'Unsupported fixture option'},501);
        return json({accepted:true});
      }
      if(action==='commands'){
        const command=data.type,config=sessionConfig(id);
        let queue=sessionQueues.get(id)||[];
        const response=value=>json({type:'response',id:data.id??null,command,success:true,data:value,error:null});
        if(!sessionAccess(id).supportedCommands.includes(command))return json({type:'response',id:data.id??null,command,success:false,error:`Test fixture: unsupported command ${command}`});
        if (command === 'get_messages' && options.pagedHistory) {
          if (controls.historyDelayMs) await new Promise(resolve => setTimeout(resolve, controls.historyDelayMs));
          if (controls.failHistoryCount > 0) { controls.failHistoryCount--; return json({type:'response',command,success:false,error:'Fixture history temporarily unavailable'}); }
          const page = messagePage(id, data.limit, controls.ignoreHistoryCursor ? null : (data.beforeId || null));
          if (!page) return json({type:'response',command,success:false,error:'beforeId not found'});
          return response(page);
        }
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
        if(command==='cancel_queued'){
          if(controls.cancelDelayMs)await new Promise(resolve=>setTimeout(resolve,controls.cancelDelayMs));
          if(controls.failCancel)return json({type:'response',id:data.id??null,command,success:false,error:'Test fixture: cancellation rejected'});
          const found=queue.find(item=>item.entryId===data.entryId),outcome=controls.cancelOutcome|| (found?'cancelled':'not_found');
          if(outcome==='cancelled'||outcome==='canceled')sessionQueues.set(id,queue.filter(item=>item.entryId!==data.entryId));
          return response({entryId:data.entryId,cancelled:outcome==='cancelled'||outcome==='canceled',outcome});
        }
        if(command==='clear_queue'){sessionQueues.set(id,[]);return response({count:0});}
        if(command==='ui_dialog_response'){
          const error=replyDialog(id,data);
          return error?json({type:'response',command,success:false,error}):response({accepted:true,requestId:data.requestId});
        }
        return response({accepted:true});
      }
    }
    return json({error:`Unknown fixture route: ${req.method} ${route}`},404);
  });
  server.on('connection',s=>{sockets.add(s);s.on('close',()=>sockets.delete(s));});
  server.on('upgrade',(req,socket)=>{const pathname=new URL(req.url,'http://localhost').pathname;if(pathname==='/api/v1/realtime')installWebSocket(req,socket);else socket.destroy();});
  await new Promise(resolve=>server.listen(port,'127.0.0.1',resolve));
  return {
    url:`http://127.0.0.1:${server.address().port}/`,requests,controls,
    setQueue(items,sessionId='dark'){sessionQueues.set(sessionId,structuredClone(items));},
    setMessages(items,sessionId='dark'){sessionMessages.set(sessionId,structuredClone(items));},
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
      for(const connection of wsConnections) for(const [subscription, id] of connection.subscriptions) if(id===sessionId) wsSend(connection,{type:'events',version:1,sessionId,subscriptionId:subscription,events:[wireEvent({seq,event})]});
      return seq;
    },
    disconnect(sessionId='dark',error=false){
      for(const connection of wsConnections) if([...connection.subscriptions.values()].includes(sessionId)) { if(error) connection.socket.destroy(); else connection.socket.end(); }
    },
    async close(){for(const s of sockets)s.destroy();await new Promise(resolve=>server.close(resolve));}
  };
}
module.exports={startFixture,fixtureCommand};

