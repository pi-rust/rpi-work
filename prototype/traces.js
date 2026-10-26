/* Local observability prototype. No Langfuse exporter, provider request or private reasoning capture. */
const tracing={sessions:{},view:{session:null,trace:null,span:null,tab:'overview'},timer:null};
const traceLabels={running:'进行中',completed:'已完成',failed:'失败',interrupted:'已停止',awaiting_settlement:'等待结算',suspended:'已挂起',declined:'未执行',settled_unknown:'待核对'};
const traceKinds={agent:'Agent',generation:'模型',tool:'工具',context:'上下文',settlement:'结算',transport:'连接'};
function traceDuration(ms){return ms==null?'—':ms===0?'0ms':ms<1?'<1ms':ms<1000?Math.round(ms)+'ms':ms<60000?(ms/1000).toFixed(2)+'s':(ms/60000).toFixed(1)+'m';}
function traceBucket(session){
 if(!tracing.sessions[session]){
  tracing.sessions[session]=[];
  if(session==='dark')tracing.sessions[session].push(traceExample());
 }
 return tracing.sessions[session];
}
function traceExample(){
 const trace={id:'demo-theme-01',session:'dark',name:'主题方案与资料检索',input:'为工作台添加深色模式，并核对现有主题与参考资料。',provider:'DeepSeek',source:'example',status:'completed',startUtc:Date.now()-60000,duration:12480,firstText:600,spans:[],seen:new Set(),metadata:{project:'rpi-work',model:'未上报',runtime:'rpi harness · 示例'}};
 trace.spans=[
  {id:'root',parent:null,kind:'agent',name:'Agent 回合',start:0,end:12480,status:'completed',input:trace.input,output:'沿用语义化颜色，为桌面与手机提供一致的深浅色主题。'},
  {id:'context',parent:'root',kind:'context',name:'准备会话上下文',start:0,end:180,status:'completed',input:'会话指令、已绑定 Package 与项目引用',output:'上下文目录已准备'},
  {id:'model-1',parent:'root',kind:'generation',name:'DeepSeek · 生成工具请求',start:180,end:3200,firstText:600,status:'completed',input:'核对现有主题文件与参考资料',output:'read / webfetch',usage:{input:12000,output:600,cache:8000,cost:0.003}},
  {id:'tools',parent:'root',kind:'agent',name:'并行工具阶段',start:3200,end:5200,status:'completed',input:'两项独立只读请求',output:'读取成功；远程参考超时，继续使用本地资料'},
  {id:'read',parent:'tools',kind:'tool',name:'read · 主题文件',start:3200,end:3600,status:'completed',input:'{"path":"assets/theme.css"}',output:'已读取现有主题变量与布局约定。'},
  {id:'web',parent:'tools',kind:'tool',name:'webfetch · 设计参考',start:3200,end:5200,status:'failed',input:'{"url":"https://example.com/theme-guide"}',output:'未取得参考资料。',error:'NETWORK_TIMEOUT · 示例超时'},
  {id:'model-2',parent:'root',kind:'generation',name:'DeepSeek · 汇总回复',start:5200,end:11920,status:'completed',input:'主题文件与工具返回摘要',output:'优先复用现有 token，验证手机布局与文字对比度。',usage:{input:20000,output:4200,cache:16000,cost:0.0154}},
  {id:'settlement',parent:'root',kind:'settlement',name:'等待运行时结算',start:11920,end:12480,status:'completed',input:'agent_end',output:'agent_settled · completed'}
 ];return trace;
}
function findTrace(id,session=state.session){return traceBucket(session).find(t=>t.id===id);}
function traceElapsed(trace){return trace.duration??Math.max(0,performance.now()-trace.startPerf);}
function traceSpanEnd(span,trace){return span.end??traceElapsed(trace);}
function traceUsage(trace){
 const generations=trace.spans.filter(s=>s.kind==='generation');
 if(!generations.length||generations.some(s=>!s.usage))return null;
 return generations.reduce((total,s)=>({input:total.input+s.usage.input,output:total.output+s.usage.output,cache:total.cache+s.usage.cache,cost:total.cost+s.usage.cost}),{input:0,output:0,cache:0,cost:0});
}
function traceStreamStart(run){
 run.startedAt=performance.now();
 const input=(state.messages[run.session]||[]).findLast(m=>m.role==='user')?.text||'';
 const trace={id:run.id,session:run.session,name:input.slice(0,40)||'Agent 回合',input,provider:run.provider,source:'local',status:'running',startPerf:run.startedAt,startUtc:Date.now(),duration:null,firstText:null,spans:[{id:'root',parent:null,kind:'agent',name:'Agent 回合',start:0,end:null,status:'running',input,output:''}],seen:new Set(),metadata:{project:state.project,model:'未上报',thinking:runtime.thinking,packages:run.resourceSnapshot?.packages||[],timing:'客户端本地演示事件'}};
 traceBucket(run.session).push(trace);updateTraceChrome();
}
function recordTraceEnvelope(run,envelope){
 const trace=findTrace(run.id,run.session);if(!trace||trace.seen.has(envelope.seq))return;
 trace.seen.add(envelope.seq);
 const event=envelope.event,at=Math.max(0,(envelope.at??performance.now())-trace.startPerf),root=trace.spans[0];
 const end=(span,status='completed')=>{if(span&&span.end==null){span.end=at;span.status=status;}};
 if(event.type==='phase'&&event.phase==='preparing'&&!trace.spans.some(s=>s.id==='context'))trace.spans.push({id:'context',parent:'root',kind:'context',name:'准备上下文',start:at,end:null,status:'running',input:'已绑定的会话资源目录',output:''});
 if(event.type==='block_start'){
  if(event.kind==='summary'){const context=trace.spans.find(s=>s.id==='context');if(context)context.output=event.text;}
  else{
   end(trace.spans.find(s=>s.id==='context'));
   trace.spans.push({id:event.id,parent:'root',kind:event.kind==='tool'?'tool':'generation',name:event.kind==='tool'?event.name:'正文流式阶段 · '+event.id,start:at,end:null,status:'running',input:event.args||trace.input,output:''});
  }
 }
 const span=trace.spans.find(s=>s.id===event.id);
 if(event.type==='text_delta'&&span){span.output+=event.delta;if(span.firstText==null)span.firstText=at;if(trace.firstText==null)trace.firstText=at;root.output=trace.spans.filter(s=>s.kind==='generation').map(s=>s.output).join('\n');}
 if(event.type==='tool_update'&&span)span.output=event.mode==='snapshot'?event.text:span.output+event.text;
 if(event.type==='block_end')end(span);
 if(event.type==='tool_end'){end(span,event.failed?'failed':'completed');if(event.failed)span.error='工具执行失败；详细错误未上报';}
 if(event.type==='agent_end'){
  trace.status='awaiting_settlement';root.status='awaiting_settlement';
  if(!trace.spans.some(s=>s.id==='settlement'))trace.spans.push({id:'settlement',parent:'root',kind:'settlement',name:'等待运行时结算',start:at,end:null,status:'running',input:'agent_end',output:''});
 }
 if(event.type==='agent_settled'){
  const status=normalizeSettlement(event.outcome);trace.status=status;trace.duration=at;trace.metadata.outcome=event.outcome;trace.metadata.pendingResume=event.pendingResume===true;
  trace.spans.forEach(s=>end(s,s.id==='root'?status:status==='completed'?'completed':status));
  root.status=status;root.error=event.error||null;
  const settlement=trace.spans.find(s=>s.id==='settlement');if(settlement)settlement.output='agent_settled · '+event.outcome;
 }
 updateTraceChrome();
}
function traceCompaction(session,before,after,preserve,started){
 const duration=performance.now()-started,id='compact-'+crypto.randomUUID();
 traceBucket(session).push({id,session,name:'上下文压缩 · 本地演示',provider:null,source:'local',status:'completed',startUtc:Date.now(),duration,firstText:null,spans:[{id:'root',parent:null,kind:'context',name:'生成本地压缩示例',start:0,end:duration,status:'completed',input:JSON.stringify({before,preserve},null,2),output:JSON.stringify({after,note:'预置摘要；未调用模型'},null,2)}],seen:new Set(),metadata:{project:state.project,timing:'本地状态操作',before,after}});
 updateTraceChrome();
}
function resetSessionTraces(session){delete tracing.sessions[session];updateTraceChrome();}
function traceRows(trace){
 const ordered=[];function walk(parent,depth){trace.spans.filter(s=>s.parent===parent).forEach(span=>{ordered.push({span,depth});walk(span.id,depth+1);});}walk(null,0);return ordered;
}
function traceStatus(status){return traceLabels[status]||status;}
function currentTrace(){return findTrace(tracing.view.trace,tracing.view.session);}
function selectedTraceSpan(trace){return trace.spans.find(s=>s.id===tracing.view.span)||trace.spans[0];}
function openSessionTrace(id){
 const traces=traceBucket(state.session),trace=traces.find(t=>t.id===id)||traces.at(-1);
 tracing.view={session:state.session,trace:trace?.id||null,span:'root',tab:'overview'};
 $('#trace-dialog').classList.toggle('trace-phone-preview',$('#app').classList.contains('phone-preview'));
 renderTraceDialog();$('#trace-dialog').showModal();$('#session-trace-trigger').setAttribute('aria-expanded','true');
}
function traceMetrics(trace){
 const usage=traceUsage(trace);
 return `<div><small>回合总耗时</small><strong data-trace-total>${traceDuration(traceElapsed(trace))}</strong></div><div><small>首段延迟</small><strong data-trace-first>${traceDuration(trace.firstText)}</strong></div><div><small>输入 / 输出 Token</small><strong>${usage?formatTokens(usage.input)+' / '+formatTokens(usage.output):'— / —'}</strong></div><div><small>费用${usage?' · 估算':''}</small><strong>${usage?'$'+usage.cost.toFixed(4):'—'}</strong></div>`;
}
function renderTraceDialog(){
 const traces=traceBucket(tracing.view.session),trace=currentTrace(),session=state.sessions.find(s=>s.id===tracing.view.session);
 $('#trace-dialog').innerHTML=`<header class="trace-header"><div><h2 id="trace-heading">会话追踪</h2><p>${escapeHTML(session?.title||'当前会话')}</p></div><button class="icon-button" data-trace-action="close" aria-label="关闭会话追踪">${icon('x')}</button></header>${trace?`<div class="trace-mobile-picker"><label for="trace-round">回合</label><select id="trace-round">${traces.map((t,i)=>`<option value="${escapeHTML(t.id)}" ${trace.id===t.id?'selected':''}>${i+1}. ${escapeHTML(t.name)}</option>`).join('')}</select></div><div class="trace-metrics" id="trace-metrics">${traceMetrics(trace)}</div><div class="trace-layout"><aside class="trace-rounds" aria-label="本会话的追踪回合"><div class="trace-rounds-caption">${traces.length} 个回合</div>${traces.map((t,i)=>`<button data-trace-select="${escapeHTML(t.id)}" ${trace.id===t.id?'aria-current="true"':''}><small>回合 ${i+1} · ${t.source==='example'?'历史示例':'本地演示'}</small><strong>${escapeHTML(t.name)}</strong><span><span data-trace-list-status="${escapeHTML(t.id)}">${traceStatus(t.status)}</span> · <b data-trace-list-time="${escapeHTML(t.id)}">${traceDuration(traceElapsed(t))}</b></span></button>`).join('')}</aside><section class="trace-main"><div class="trace-waterfall"><div class="trace-section-heading"><strong>调用时间线</strong><span data-trace-step-count>${trace.source==='example'?'历史示例数据':'本地事件计时'} · ${trace.spans.length} 步</span></div><div class="trace-axis"><span>步骤</span><div><span>0</span><span data-trace-axis-end>${traceDuration(traceElapsed(trace))}</span></div><span>耗时</span></div><div id="trace-waterfall-rows"></div></div><section class="trace-details" aria-label="调用详情"><div class="trace-detail-title"><strong id="trace-span-name"></strong><span id="trace-span-status"></span></div><div class="trace-detail-tabs" role="group" aria-label="追踪详情分类">${[['overview','概览'],['io','输入 / 输出'],['metadata','元数据']].map(([id,label])=>`<button data-trace-tab="${id}" aria-pressed="${tracing.view.tab===id}">${label}</button>`).join('')}</div><div id="trace-detail-body"></div></section></section></div><footer class="trace-footer"><code title="${escapeHTML(trace.id)}">${escapeHTML(trace.id)}</code><span>${trace.source==='example'?'示例数据 · 费用为估算':'客户端计时 · 未上报用量显示 —'}</span></footer>`:'<div class="trace-empty">'+icon('chart')+'<h3>此会话暂无追踪</h3><p>发送消息或运行流式演示后，查看本轮调用与耗时。</p></div>'}`;
 if(trace){updateTraceWaterfall(trace);renderTraceSpanDetails(trace);}
}
function updateTraceWaterfall(trace){
 const target=$('#trace-waterfall-rows');if(!target)return;
 const total=Math.max(1,traceElapsed(trace));
 for(const {span,depth} of traceRows(trace)){
  let row=$$('[data-trace-span]',target).find(el=>el.dataset.traceSpan===span.id);
  if(!row){target.insertAdjacentHTML('beforeend',`<button class="trace-span-row" data-trace-span="${escapeHTML(span.id)}" style="--trace-depth:${depth}"><span class="trace-span-label"><i class="trace-kind ${span.kind}">${traceKinds[span.kind]||span.kind}</i><span>${escapeHTML(span.name)}</span></span><span class="trace-span-track"><i></i></span><span class="trace-span-duration"></span></button>`);row=target.lastElementChild;}
  row.setAttribute('aria-pressed',String(span.id===tracing.view.span));row.dataset.status=span.status;row.title=span.name+' · '+traceStatus(span.status);
  $('.trace-span-duration',row).textContent=traceDuration(traceSpanEnd(span,trace)-span.start);
  const bar=$('.trace-span-track>i',row);bar.style.left=Math.min(100,span.start/total*100)+'%';bar.style.width=Math.max(.6,Math.min(100-span.start/total*100,(traceSpanEnd(span,trace)-span.start)/total*100))+'%';
 }
 $('[data-trace-axis-end]').textContent=traceDuration(total);
}
function renderTraceSpanDetails(trace){
 const span=selectedTraceSpan(trace),usage=span.id==='root'?traceUsage(trace):span.usage;
 $('#trace-span-name').textContent=span.name;$('#trace-span-status').textContent=traceStatus(span.status);$('#trace-span-status').dataset.status=span.status;
 $$('[data-trace-tab]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.traceTab===tracing.view.tab)));
 if(tracing.view.tab==='io')$('#trace-detail-body').innerHTML=`<p class="trace-error" id="trace-detail-error" ${span.error?'':'hidden'}></p><div class="trace-io"><section><h4>输入</h4><pre id="trace-detail-input"></pre></section><section><h4>输出</h4><pre id="trace-detail-output"></pre></section></div>`;
 else if(tracing.view.tab==='metadata')$('#trace-detail-body').innerHTML='<pre class="trace-metadata" id="trace-detail-metadata"></pre>';
 else $('#trace-detail-body').innerHTML=`<dl class="trace-overview"><div><dt>状态</dt><dd id="trace-detail-state"></dd></div><div><dt>耗时</dt><dd id="trace-detail-duration"></dd></div><div><dt>开始偏移</dt><dd>${traceDuration(span.start)}</dd></div><div><dt>Provider</dt><dd>${escapeHTML(trace.provider||'—')}</dd></div><div><dt>输入 / 输出</dt><dd>${usage?formatTokens(usage.input)+' / '+formatTokens(usage.output):'— / —'}</dd></div><div><dt>缓存命中</dt><dd>${usage&&usage.input>0?(usage.cache/usage.input*100).toFixed(0)+'%':'—'}</dd></div><div><dt>首段等待</dt><dd id="trace-detail-first">${traceDuration(span.id==='root'?trace.firstText:span.firstText==null?null:span.firstText-span.start)}</dd></div><div><dt>计时来源</dt><dd>${trace.source==='example'?'内置历史示例':'客户端本地事件'}</dd></div></dl><p class="trace-error" id="trace-detail-error" ${span.error?'':'hidden'}></p><p class="trace-hint">总耗时采用起止时间差；并行步骤不相加。原型未连接模型或 Langfuse。</p>`;
 updateTraceSpanDetails(trace);
}
function updateTraceSpanDetails(trace){
 const span=selectedTraceSpan(trace);
 if($('#trace-detail-error')){$('#trace-detail-error').hidden=!span.error;$('#trace-detail-error').textContent=span.error||'';}
 if($('#trace-detail-first'))$('#trace-detail-first').textContent=traceDuration(span.id==='root'?trace.firstText:span.firstText==null?null:span.firstText-span.start);
 if($('#trace-detail-duration'))$('#trace-detail-duration').textContent=traceDuration(traceSpanEnd(span,trace)-span.start);
 if($('#trace-detail-state'))$('#trace-detail-state').textContent=traceStatus(span.status);
 if($('#trace-detail-input')&&$('#trace-detail-input').textContent!==(span.input||'未上报'))$('#trace-detail-input').textContent=span.input||'未上报';
 if($('#trace-detail-output'))patchStreamText($('#trace-detail-output'),span.output||'未上报');
 const metadata=JSON.stringify({traceId:trace.id,spanId:span.id,parentSpanId:span.parent,sessionId:trace.session,type:span.kind,provider:trace.provider,source:trace.source,startTime:new Date(trace.startUtc).toISOString(),startOffsetMs:span.start,endOffsetMs:span.end??null,status:span.status,...trace.metadata},null,2);
 if($('#trace-detail-metadata')&&$('#trace-detail-metadata').textContent!==metadata)$('#trace-detail-metadata').textContent=metadata;
 $('#trace-span-status').textContent=traceStatus(span.status);$('#trace-span-status').dataset.status=span.status;
}
function updateTraceChrome(){
 const traces=traceBucket(state.session),latest=traces.at(-1),trigger=$('#session-trace-trigger');
 if(trigger){$('span',trigger).textContent='追踪'+(latest?' · '+traceDuration(traceElapsed(latest)):'');trigger.title=latest?'查看会话追踪 · '+traceStatus(latest.status):'查看会话追踪';}
 $$('[data-trace-run]').forEach(button=>{const trace=findTrace(button.dataset.traceRun);if(trace)$('span',button).textContent='追踪 · '+traceDuration(traceElapsed(trace));});
 if($('#trace-dialog').open){
  const trace=currentTrace();if(!trace)return;
  $('[data-trace-total]').textContent=traceDuration(traceElapsed(trace));$('[data-trace-first]').textContent=traceDuration(trace.firstText);$('[data-trace-step-count]').textContent=(trace.source==='example'?'历史示例数据':'本地事件计时')+' · '+trace.spans.length+' 步';updateTraceWaterfall(trace);updateTraceSpanDetails(trace);
  $$('[data-trace-list-time]').forEach(el=>{const t=findTrace(el.dataset.traceListTime,tracing.view.session);if(t){el.textContent=traceDuration(traceElapsed(t));const label=$$('[data-trace-list-status]').find(e=>e.dataset.traceListStatus===t.id);if(label)label.textContent=traceStatus(t.status);}});
 }
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button)return;
 if(button.id==='session-trace-trigger')openSessionTrace();
 if(button.dataset.traceRun)openSessionTrace(button.dataset.traceRun);
 if(button.dataset.traceSelect){tracing.view.trace=button.dataset.traceSelect;tracing.view.span='root';renderTraceDialog();}
 if(button.dataset.traceSpan){tracing.view.span=button.dataset.traceSpan;updateTraceWaterfall(currentTrace());renderTraceSpanDetails(currentTrace());}
 if(button.dataset.traceTab){tracing.view.tab=button.dataset.traceTab;renderTraceSpanDetails(currentTrace());}
 if(button.dataset.traceAction==='close')$('#trace-dialog').close();
});
document.addEventListener('change',event=>{if(event.target.id==='trace-round'){tracing.view.trace=event.target.value;tracing.view.span='root';renderTraceDialog();}});
$('#trace-dialog').addEventListener('keydown',event=>{if(event.key==='Escape')event.stopPropagation();});
$('#trace-dialog').addEventListener('close',()=>$('#session-trace-trigger').setAttribute('aria-expanded','false'));
document.addEventListener('DOMContentLoaded',()=>{
 $('.composer-foot>span:last-child').insertAdjacentHTML('beforebegin',`<button id="session-trace-trigger" class="session-trace-trigger" aria-label="查看会话追踪与耗时" aria-haspopup="dialog" aria-controls="trace-dialog" aria-expanded="false">${icon('chart')}<span>追踪</span></button>`);
 updateTraceChrome();tracing.timer=setInterval(updateTraceChrome,500);
});
