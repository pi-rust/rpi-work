/* Mobile session navigation uses the same session state and draft handling as the sidebar. */
const recentSessionIds=[];
function renderQuickSessionTrigger(){
 const session=state.sessions.find(s=>s.id===state.session);if(!session)return;
 const previous=recentSessionIds.indexOf(session.id);if(previous>=0)recentSessionIds.splice(previous,1);recentSessionIds.unshift(session.id);
 $('#quick-session-project').textContent=metaAgent.projects.find(p=>p.project===session.project)?.name||session.project;
 $('#quick-session-title').textContent=session.title;
 $('#quick-session-trigger').setAttribute('aria-label',`切换会话，当前：${session.title}`);
}
function quickSessionSwitcher(){
 closeSidebar();
 showModal('切换会话',`<div class="quick-session-toolbar"><span>最近访问 · 所有项目</span><button id="quick-session-new" ${state.running?'disabled':''}>${icon('plus')}新建会话</button></div><label class="quick-session-search" for="quick-session-search">${icon('search')}<input id="quick-session-search" aria-label="搜索会话或项目" type="search" placeholder="搜索会话或项目…" autocomplete="off"></label>${state.running?'<p class="quick-session-notice">当前演示正在生成，停止或完成后可切换会话。</p>':''}<div class="quick-session-results" id="quick-session-results"></div><div id="quick-session-count" class="quick-session-count" role="status"></div>`);
 $('#modal').classList.add('session-switcher-modal');
 $('#quick-session-trigger').setAttribute('aria-expanded','true');
 renderQuickSessionList('');
}
function renderQuickSessionList(query){
 const q=query.trim().toLocaleLowerCase();
 const ordered=[...recentSessionIds,...state.sessions.map(s=>s.id)].filter((id,index,ids)=>ids.indexOf(id)===index);
 const sessions=ordered.map(id=>state.sessions.find(s=>s.id===id)).filter(s=>s&&(s.title+' '+s.project+' '+projectLabel(s.project)).toLocaleLowerCase().includes(q));
 $('#quick-session-results').innerHTML=sessions.map(s=>{
  const current=s.id===state.session,draft=current?$('#prompt').value:runtime.sessions[s.id]?.textDraft;
  return `<button class="quick-session-row ${current?'current':''}" data-quick-session="${escapeHTML(s.id)}" ${current?'aria-current="true"':''} ${state.running&&!current?'disabled':''}>${icon('message')}<span><strong>${escapeHTML(s.title)}</strong><small>${escapeHTML(projectLabel(s.project))}${draft?.trim()?' · 有草稿':''}</small></span>${current?`<em>当前</em>${icon('check')}`:icon('chevron-right')}</button>`;
 }).join('')||'<p class="quick-session-empty">没有匹配的会话，试试其他名称或项目。</p>';
 $('#quick-session-count').textContent=`${sessions.length} 个会话`;
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button||button.disabled)return;
 if(button.id==='quick-session-trigger')quickSessionSwitcher();
 if(button.id==='quick-session-new')newTask();
 if(button.dataset.quickSession){
  if(state.running&&button.dataset.quickSession!==state.session)return;
  if(button.dataset.quickSession!==state.session)selectSession(button.dataset.quickSession);
  $('#quick-session-trigger').setAttribute('aria-expanded','false');$('#modal').close();$('#quick-session-trigger').focus();
 }
});
document.addEventListener('input',event=>{if(event.target.id==='quick-session-search')renderQuickSessionList(event.target.value);});
document.addEventListener('keydown',event=>{
 if(event.key==='Escape'&&$('#modal').open&&$('#modal').classList.contains('session-switcher-modal')){
  event.preventDefault();$('#quick-session-trigger').setAttribute('aria-expanded','false');$('#modal').close();
 }
});
$('#modal').addEventListener('cancel',()=>$('#quick-session-trigger').setAttribute('aria-expanded','false'));
$('#modal').addEventListener('close',()=>{if(!$('#modal').open)$('#quick-session-trigger').setAttribute('aria-expanded','false');});
