/* Local event-stream simulator. No model, SSE connection, shell or filesystem I/O.
 * These are normalized UI events, not the rpi wire protocol. See docs/streaming.md.
 */
const streams={active:new Map(),unread:0};
const streamPhases={accepted:'请求已接收',preparing:'准备上下文',text:'正在生成',tool:'工具执行中',awaiting_settlement:'等待运行时结算',cancelling:'正在请求停止',completed:'本轮已完成',interrupted:'已停止演示生成',failed:'本轮失败',suspended:'已挂起，等待恢复',declined:'本轮未执行',settled_unknown:'结果待核对'};
function latestStream(){return (state.messages[state.session]||[]).findLast(m=>m.role==='stream');}
function streamTerminal(run){return ['completed','interrupted','failed','suspended','declined','settled_unknown'].includes(run.phase);}
function normalizeSettlement(outcome){if(['completed','compacted'].includes(outcome))return 'completed';if(['aborted','interrupted'].includes(outcome))return 'interrupted';if(outcome==='failed')return 'failed';if(outcome==='suspended')return 'suspended';if(['declined','skipped'].includes(outcome))return 'declined';return 'settled_unknown';}
function streamStatus(run){return streamPhases[run.phase]||run.phase;}
function streamById(id){for(const messages of Object.values(state.messages)){const r=messages.find(m=>m.role==='stream'&&m.id===id);if(r)return r;}}
function streamButton(action,label,disabled=false){return `<button data-stream="${action}" ${disabled?'disabled':''}>${label}</button>`;}
function renderStreamMessage(run){
  return `<article class="message stream-message" id="${run.id}" data-run-id="${run.id}"><div class="message-avatar agent">r↗</div><div class="message-content"><div class="message-author">rpi <span class="via">${escapeHTML(run.provider)} · 本地流式演示</span></div><div class="stream-run-state" data-phase="${run.phase}" role="status">${escapeHTML(streamStatus(run))}</div><button class="run-trace-link" data-trace-run="${escapeHTML(run.id)}" aria-label="查看本轮追踪与耗时">${icon('chart')}<span>追踪</span></button><div class="stream-blocks">${run.blocks.map(renderStreamBlock).join('')}</div><div class="stream-result">${streamResult(run)}</div><details class="stream-controls" ${run.controlsOpen?'open':''}><summary>流式演示控制 <span>本地事件，不调用工具</span></summary><div>${streamButton('disconnect','模拟断线',streamTerminal(run)||run.transport!=='online')}${streamButton('reconnect','立即重连',streamTerminal(run)||run.transport==='online')}${streamButton('fail','模拟运行失败',streamTerminal(run)||run.phase==='cancelling')}${streamButton('resync','模拟恢复失效',streamTerminal(run))}<label><input type="checkbox" data-stream="hold" ${run.hold?'checked':''} ${streamTerminal(run)?'disabled':''}>暂缓最终结算</label>${streamButton('settle','接收结算',run.phase!=='awaiting_settlement'||!run.held)}</div><small>断线只影响接收；停止需等待模拟宿主确认。恢复失效会重建快照，不重发原指令。</small></details></div></article>`;
}
function renderStreamBlock(block){
  if(block.kind==='text')return `<div class="stream-text ${block.done?'':'is-streaming'}" data-block-id="${block.id}">${escapeHTML(block.text)}</div>`;
  if(block.kind==='summary')return `<details class="stream-summary" data-block-id="${block.id}"><summary>${icon('sparkles')}准备摘要 <span>可公开的状态说明</span></summary><p>${escapeHTML(block.text)}</p></details>`;
  return `<details class="stream-tool" data-block-id="${block.id}" ${block.open?'open':''}><summary>${icon('terminal')}<strong>${escapeHTML(block.name)}</strong><span class="stream-tool-status">${block.status==='done'?'已完成':block.status==='cancelled'?'已取消':block.status==='failed'?'失败':'输出接收中'}</span>${icon('chevron-down')}</summary><div class="stream-tool-body"><code>${escapeHTML(block.args)}</code><pre>${escapeHTML(block.text)}</pre><small>演示输出 · 不执行命令</small></div></details>`;
}
function streamResult(run){
  const note=run.phase==='completed'?`已收到 agent_settled · ${run.outcome==='compacted'?'compacted':'completed'}。Token / 缓存 / 费用尚未上报，显示 —；页脚保留此前已结算样例。`:run.phase==='interrupted'?'已停止演示生成。部分正文与工具输出已保留；未完成的工具已标记取消。':run.phase==='failed'?'模拟宿主返回失败，部分内容已保留。可补充指令后发起新一轮；不会自动重试工具。':run.phase==='awaiting_settlement'?'已收到 agent_end；等待 agent_settled 确认本轮结果。':run.phase==='cancelling'?'停止请求已发出，等待模拟宿主确认。':'';
  const extra=run.phase==='suspended'?'本轮已挂起；不会自动重发原指令。':run.phase==='declined'?'宿主未执行此操作，可能是压缩未获采纳或无需恢复。':run.phase==='settled_unknown'?'宿主返回了未识别的结果，需要同步状态后核对。':run.outcome==='compacted'?'上下文压缩已结算。':run.pendingResume?'宿主标记此轮存在可恢复工作；尚未自动恢复。':'';
  return (note?`<p>${note}</p>`:'')+(extra?`<p>${extra}</p>`:'');
}
function renderStreamChrome(){
  if(!$('#stream-livebar'))$('#queue-shelf').insertAdjacentHTML('beforebegin','<div id="stream-livebar" class="stream-livebar" hidden></div>');
  const run=latestStream(),bar=$('#stream-livebar');bar.hidden=!run||state.tab!=='conversation';
  if(!run)return;
  const transport=run.transport==='online'?'已连接':run.transport==='resyncing'?'正在同步快照':'连接中断 · 正在重连';
  bar.innerHTML=`<span class="stream-connection ${run.transport==='online'?'':'disconnected'}"><i></i>${transport}</span><span class="stream-live-count">${run.chunks} 片段 · ${run.lastSeq} seq${run.recovered?' · 已恢复':''}</span><button data-stream="follow" ${!streams.unread?'hidden':''}>新进展 ↓</button><small>模拟</small>`;
}
function streamPinned(){const el=$('#task-body');return el.scrollHeight-el.clientHeight-el.scrollTop<64;}
function followStream(){const el=$('#task-body');el.scrollTop=el.scrollHeight;streams.unread=0;renderStreamChrome();}
function patchStreamText(node,text){const prior=node.textContent;if(text.startsWith(prior)){if(text.length>prior.length)node.append(document.createTextNode(text.slice(prior.length)));}else node.textContent=text;}

// Incremental DOM projection: no full transcript re-render on each fragment.
function paintStream(run){
  if(run.session!==state.session)return;
  const root=document.getElementById(run.id),body=$('#task-body'),top=body.scrollTop,pinned=streamPinned();
  if(root){
    const status=$('.stream-run-state',root);status.textContent=streamStatus(run);status.dataset.phase=run.phase;
    const blocks=$('.stream-blocks',root);
    for(const block of run.blocks){
      let node=$(`[data-block-id="${block.id}"]`,blocks);
      if(!node){blocks.insertAdjacentHTML('beforeend',renderStreamBlock(block));continue;}
      if(block.kind==='text'){patchStreamText(node,block.text);node.classList.toggle('is-streaming',!block.done);}
      else if(block.kind==='tool'){
        patchStreamText($('pre',node),block.text);
        $('.stream-tool-status',node).textContent=block.status==='done'?'已完成':block.status==='cancelled'?'已取消':block.status==='failed'?'失败':'输出接收中';
      }
    }
    $('.stream-result',root).innerHTML=streamResult(run);
    const disabled=streamTerminal(run),control=action=>$(`[data-stream="${action}"]`,root);
    control('disconnect').disabled=disabled||run.transport!=='online';control('reconnect').disabled=disabled||run.transport==='online';control('fail').disabled=disabled||run.phase==='cancelling';control('resync').disabled=disabled||run.phase==='cancelling';control('hold').disabled=disabled;control('hold').checked=run.hold;control('settle').disabled=run.phase!=='awaiting_settlement'||!run.held;
  }
  if(root&&!pinned)streams.unread++;
  updateStatus();renderStreamChrome();
  if(root){if(pinned){body.scrollTop=body.scrollHeight;streams.unread=0;}else body.scrollTop=top;}
}

// UI projection accepts ordered, idempotent envelopes scoped to this run.
// Actual rpi message_update contains message snapshots; the adapter must normalize them.
function reduceStream(run,envelope){
  if(envelope.runId!==run.id||envelope.seq<=run.lastSeq||streamTerminal(run))return false;
  if(envelope.seq!==run.lastSeq+1)throw Error('Stream gap: snapshot required');
  run.lastSeq=envelope.seq;run.applied.push(envelope.seq);
  const e=envelope.event;
  if(e.type==='phase')run.phase=e.phase;
  else if(e.type==='block_start')run.blocks.push({id:e.id,kind:e.kind,text:e.text||'',name:e.name,args:e.args,status:'running',open:true,done:false});
  else if(e.type==='text_delta'){
    const block=run.blocks.find(b=>b.id===e.id);block.text+=e.delta;run.chunks++;run.phase='text';
  }else if(e.type==='block_end')run.blocks.find(b=>b.id===e.id).done=true;
  else if(e.type==='tool_update'){
    const block=run.blocks.find(b=>b.id===e.id);
    block.text=e.mode==='snapshot'?e.text:block.text+e.text;run.chunks++;run.phase='tool';
  }else if(e.type==='tool_end'){
    const block=run.blocks.find(b=>b.id===e.id);block.status=e.failed?'failed':'done';
  }else if(e.type==='agent_end')run.phase='awaiting_settlement';
  else if(e.type==='agent_settled'){
    run.outcome=e.outcome;run.pendingResume=e.pendingResume===true;run.error=e.error||null;run.phase=normalizeSettlement(e.outcome);
    run.blocks.forEach(b=>{b.done=true;if(b.kind==='tool'&&b.status==='running')b.status=run.phase==='failed'?'failed':'cancelled';});
    run.finishedAt=performance.now();if(run.session===state.session)state.running=false;
  }
  if(typeof recordTraceEnvelope==='function')recordTraceEnvelope(run,envelope);
  return true;
}
function receiveStream(run,envelope){
  if(streamTerminal(run)||run.phase==='cancelling')return;
  if(run.transport!=='online'){run.pending.push(envelope);return;}
  if(envelope.event.type==='agent_settled'&&run.hold){run.held=envelope;paintStream(run);return;}
  if(reduceStream(run,envelope)){
    if(streamTerminal(run)){clearStreamTimers(run);streams.active.delete(run.id);renderRuntimeFooter();renderQueueShelf();}
    paintStream(run);
  }
}
function streamSchedule(run,callback,delay){const timer=setTimeout(()=>{run.timers.delete(timer);if(streams.active.has(run.id))callback();},delay);run.timers.add(timer);return timer;}
function clearStreamTimers(run){run.timers.forEach(clearTimeout);run.timers.clear();}
function disposeStreams(){for(const run of streams.active.values())clearStreamTimers(run);streams.active.clear();streams.unread=0;}
function buildStreamScript(mode,tools){
  const script=[],emit=(at,event)=>script.push({at,event}),text=(at,id,value)=>{
    emit(at,{type:'block_start',id,kind:'text'});
    const fragments=value.match(/.{1,10}/gu)||[];
    fragments.forEach((delta,i)=>emit(at+90*(i+1),{type:'text_delta',id,delta}));
    emit(at+90*(fragments.length+1),{type:'block_end',id});return at+90*(fragments.length+1);
  };
  emit(100,{type:'phase',phase:'preparing'});
  emit(220,{type:'block_start',id:'summary',kind:'summary',text:'准备示例项目上下文与工具列表。这是可公开的状态摘要，不展示模型内部思维链。'});
  let at=text(450,'intro','我会先查看主题配置，再检查桌面与手机布局。下面逐步展示正文和工具返回的结果。');
  if(tools.includes('read')){at+=150;emit(at,{type:'phase',phase:'tool'});emit(at,{type:'block_start',kind:'tool',id:'read',name:'read · 主题配置',args:'src/theme.rs · 只读示例'});
  emit(at+250,{type:'tool_update',id:'read',mode:'snapshot',text:'读取主题配置…\n'});
  emit(at+500,{type:'tool_update',id:'read',mode:'snapshot',text:'读取主题配置…\n已找到 light / dark 语义变量。\n'});
  emit(at+700,{type:'tool_end',id:'read'});at+=850;}
  if(mode!=='规划'&&tools.includes('powershell')){
    at+=900;emit(at,{type:'block_start',kind:'tool',id:'verify',name:'shell · 布局检查',args:'模拟输出 · 不执行脚本或测试'});
    ['[1/3] 桌面布局通过（示例）\n','[2/3] 窄屏布局通过（示例）\n','[3/3] 主题对比度通过（示例）\n'].forEach((line,i)=>emit(at+250*(i+1),{type:'tool_update',id:'verify',mode:'append',text:line}));
    emit(at+900,{type:'tool_end',id:'verify'});at+=1000;
  }else at+=850;
  at=text(at,'answer',mode==='规划'?'演示回复：我会先检查项目结构，列出实现步骤与验证方法，确认方向后再执行。正式版本将由 rpi-plan-mode 提供规划能力。':'演示回复：收到你的补充。我会结合当前项目上下文继续处理，并把进度、工具调用和变更显示在这里。附件仅在本地展示，当前原型没有连接模型或执行工具。');
  emit(at+120,{type:'agent_end'});emit(at+850,{type:'agent_settled',outcome:'completed'});
  return script;
}
function startStreamRun(){
  const run={role:'stream',id:'run-'+crypto.randomUUID(),session:state.session,provider:state.model,resourceSnapshot:captureSessionResources(),phase:'accepted',transport:'online',blocks:[],lastSeq:0,chunks:0,applied:[],pending:[],timers:new Set(),history:[],hold:false,held:null,recovered:false};
  traceStreamStart(run);
  (state.messages[state.session]||=[]).push(run);streams.active.set(run.id,run);streams.unread=0;state.tab='conversation';state.running=true;renderTab();followStream();
  buildStreamScript(state.mode,run.resourceSnapshot.tools).forEach(({at,event},i)=>streamSchedule(run,()=>{const envelope={runId:run.id,seq:i+1,event,at:performance.now()};run.history.push(envelope);receiveStream(run,envelope);},at));
  return run;
}
function startStreamDemo(){
  if(state.running){toast('当前已在流式生成，可展开对话内的演示控制');return;}
  (state.messages[state.session]||=[]).push({role:'user',text:'演示流式消息：逐步生成正文、追加工具输出，并展示断线恢复和结算。'});
  navigate('workspace');startStreamRun();
}
function stopStream(){
  const run=latestStream();if(!run||streamTerminal(run)||run.phase==='cancelling')return;
  clearStreamTimers(run);run.pending=[];run.held=null;run.transport='online';run.phase='cancelling';paintStream(run);
  streamSchedule(run,()=>{
    reduceStream(run,{runId:run.id,seq:run.lastSeq+1,event:{type:'agent_settled',outcome:'aborted'}});
    streams.active.delete(run.id);renderRuntimeFooter();renderQueueShelf();paintStream(run);
  },240);
}
function disconnectStream(run){
  if(streamTerminal(run)||run.transport!=='online')return;
  run.transport='reconnecting';paintStream(run);
  run.reconnectTimer=streamSchedule(run,()=>reconnectStream(run),1600);
}
function reconnectStream(run){
  if(streamTerminal(run)||run.transport==='online')return;
  clearTimeout(run.reconnectTimer);run.timers.delete(run.reconnectTimer);run.transport='online';run.recovered=true;
  // Deliberate replay of the last envelope exercises sequence deduplication.
  const replay=run.history.find(e=>e.seq===run.lastSeq),pending=run.pending.splice(0);
  if(replay)receiveStream(run,replay);
  pending.forEach(e=>receiveStream(run,e));paintStream(run);
}
function resyncStream(run){
  if(streamTerminal(run)||run.phase==='cancelling')return;
  clearTimeout(run.reconnectTimer);run.transport='resyncing';paintStream(run);
  streamSchedule(run,()=>{
    // Rebuild from a mock snapshot source, never concatenate it onto the old text.
    const hold=run.hold,snapshot={...run,blocks:[],lastSeq:0,chunks:0,applied:[],phase:'accepted'};
    run.held=null;
    for(const e of run.history){if(e.event.type==='agent_settled'&&hold){run.held=e;continue;}reduceStream(snapshot,e);}
    Object.assign(run,{blocks:snapshot.blocks,lastSeq:snapshot.lastSeq,chunks:snapshot.chunks,applied:snapshot.applied,phase:snapshot.phase,pending:[],transport:'online',recovered:true});
    if(streamTerminal(run)){clearStreamTimers(run);streams.active.delete(run.id);state.running=false;renderRuntimeFooter();renderQueueShelf();}
    const top=$('#task-body').scrollTop,pinned=streamPinned();if(state.tab==='conversation'){renderConversation();if(pinned)followStream();else $('#task-body').scrollTop=top;}paintStream(run);
  },650);
}
document.addEventListener('click',event=>{
  const button=event.target.closest('button[data-stream]');if(!button||button.disabled)return;
  const action=button.dataset.stream,run=streamById(button.closest('[data-run-id]')?.dataset.runId)||latestStream();
  if(action==='demo'){if($('#modal').open)$('#modal').close();startStreamDemo();return;}if(action==='follow'){followStream();return;}if(!run)return;
  if(action==='disconnect')disconnectStream(run);
  if(action==='reconnect')reconnectStream(run);
  if(action==='resync')resyncStream(run);
  if(action==='fail'&&!streamTerminal(run)){
    clearStreamTimers(run);run.transport='online';run.pending=[];run.held=null;
    reduceStream(run,{runId:run.id,seq:run.lastSeq+1,event:{type:'agent_settled',outcome:'failed'}});
    streams.active.delete(run.id);renderRuntimeFooter();renderQueueShelf();paintStream(run);
  }
  if(action==='settle'&&run.held){const envelope=run.held;run.hold=false;run.held=null;receiveStream(run,envelope);}
});
document.addEventListener('change',event=>{if(event.target.matches('input[data-stream="hold"]')){const run=streamById(event.target.closest('[data-run-id]').dataset.runId);run.hold=event.target.checked;if(!run.hold&&run.held){const e=run.held;run.held=null;receiveStream(run,e);}}});
document.addEventListener('toggle',event=>{if(event.target.matches?.('.stream-controls')){const run=streamById(event.target.closest('[data-run-id]').dataset.runId);if(run)run.controlsOpen=event.target.open;}if(event.target.matches?.('.stream-tool')){const run=streamById(event.target.closest('[data-run-id]').dataset.runId),block=run?.blocks.find(b=>b.id===event.target.dataset.blockId);if(block)block.open=event.target.open;}},true);
$('#task-body').addEventListener('scroll',()=>{if(streams.unread&&streamPinned()){streams.unread=0;renderStreamChrome();}},{passive:true});
window.addEventListener('beforeunload',disposeStreams);
