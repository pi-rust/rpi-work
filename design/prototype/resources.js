/* Acquisition and activation design only. Download/install target the simulated PC node, never a real plugin host. */
const extensionCatalog=[
 ['rpi-plan-mode','checklist','先梳理计划，再进入执行。把复杂工作拆成可确认的步骤。'],
 ['rpi-permission-gate','shield','拦截 bash / write / edit；按调用请求确认。'],
 ['rpi-permissions','shield','权限规则存储与查询；单独安装不等于已启用执行拦截。'],
 ['rpi-todo','check','追踪任务步骤，让长时间工作始终保持方向。'],
 ['rpi-web','layout','连接原生 Agent，提供会话、文件与 Git 预览。'],
 ['rpi-mcp-adapter','link','接入 MCP 工具，把已有服务加入工作流程。'],
 ['rpi-subagents','blocks','为子任务提供受限的独立 Agent 能力。']
].map(([id,glyph,description])=>({id,glyph,description}));
const preinstalledExtensions=['rpi-plan-mode','rpi-permissions','rpi-todo','rpi-web'];
const preinstalledSkills=skillSamples.filter(skill=>!skill.packageId).map(skill=>skill.id);
const resourceStore={extensions:{downloaded:new Set(preinstalledExtensions),installed:new Set(preinstalledExtensions)},skills:{downloaded:new Set(preinstalledSkills),installed:new Set(preinstalledSkills)},bindings:{},pending:new Set()};
skillSamples.push({id:'catalog-api-review',name:'api-contract-review',scope:'global',source:'技能目录',path:'<node>/skills/api-contract-review/SKILL.md',status:'available',description:'检查 API 契约、错误响应与版本兼容性。下载并安装后，可在会话中启用。',trigger:'审阅 API 契约时手动指定。',content:'---\nname: api-contract-review\ndescription: Review API contracts and compatibility.\n---\n\n# API contract review\n\nInspect request/response contracts and error handling. Report compatibility risks with file references.\n\nPrototype catalogue sample.',refs:[],auto:false});
function standaloneSkillBindings(){return resourceStore.bindings[state.session]||=(new Set(preinstalledSkills));}
function resourceSkillReady(skill){
 if(skill.packageId){const version=skill.packageVersion||packageSession().bindings[skill.packageId]||packageCatalog.find(p=>p.id===skill.packageId)?.version;const key=skill.packageId+'@'+version;return packageStore.downloaded.has(key)&&packageStore.installed.has(key);}
 return resourceStore.skills.downloaded.has(skill.id)&&resourceStore.skills.installed.has(skill.id);
}
function resourceSkillEnabled(skill){return skill.packageId?packageActive(skill.packageId):standaloneSkillBindings().has(skill.id);}
function resourceSteps(downloaded,installed,enabled){return `<ol class="resource-steps" aria-label="下载、安装与启用状态">${[['下载',downloaded],['安装',installed],['启用',enabled]].map(([label,done],index)=>`<li class="${done?'done':''}"><i>${done?icon('check'):index+1}</i>${label}</li>`).join('')}</ol>`;}
function resourceRefresh(){
 if(state.page==='extensions')renderExtensions();
 if(state.page==='skills')renderSkillCatalogue();
 if(state.page==='packages')renderPackageLibrary();
 renderCapabilitiesUI();
 if($('#modal').open&&$('#modal').querySelector('[data-resource-action]')&&capabilities.skillDetailId)skillDetail(capabilities.skillDetailId,capabilities.skillDetailTab);
}
function downloadNodeResource(kind,id){
 const store=resourceStore[kind],resource=kind==='extensions'?extensionCatalog.find(item=>item.id===id):skillById(id);if(!store||!resource||store.downloaded.has(id))return;
 const key=kind+':'+id;if(resourceStore.pending.has(key))return;resourceStore.pending.add(key);resourceRefresh();
 setTimeout(()=>{resourceStore.pending.delete(key);store.downloaded.add(id);resourceRefresh();toast('已模拟下载到这台电脑；安装后仍需明确启用');},300);
}
function installNodeResource(kind,id){
 const store=resourceStore[kind];if(!store?.downloaded.has(id)||resourceStore.pending.has(kind+':'+id)){toast('请先下载到执行节点');return;}
 store.installed.add(id);resourceRefresh();toast('已模拟安装；尚未启用');
}
function toggleExtension(id){
 const store=resourceStore.extensions;if(!store.downloaded.has(id)||!store.installed.has(id)){toast('请先下载并安装扩展');return;}
 if(sessionConfigBusy()){toast('请在本轮结束后调整扩展');return;}
 const extension=extensionCatalog.find(item=>item.id===id);
 if(!state.enabled[id]&&extension?.personalResourceId)extensionCatalog.filter(item=>item.personalResourceId===extension.personalResourceId&&item.id!==id).forEach(item=>{state.enabled[item.id]=false;});
 state.enabled[id]=!state.enabled[id];renderExtensions();toast(state.enabled[id]?'已启用扩展示例':'已停用扩展示例');
}
function renderExtensionCatalogue(){
 const store=resourceStore.extensions;
 $('#page-view').innerHTML=`<div class="overline">MAKE IT YOURS.</div><div class="page-heading"><div><h1>扩展</h1><p>先下载到执行节点，再安装并启用。</p></div></div><div class="info-banner">${icon('info')}下载、安装和启用分开进行；已启用项是预先下载并安装的示例。以下操作均为模拟，不执行真实扩展安装。</div><div class="card-grid">${extensionCatalog.map(extension=>{
  const downloaded=store.downloaded.has(extension.id),installed=store.installed.has(extension.id),enabled=installed&&downloaded&&!!state.enabled[extension.id],pending=resourceStore.pending.has('extensions:'+extension.id);
  return `<article class="extension-card" data-extension-card="${extension.id}"><div class="extension-card-top"><span class="extension-icon">${icon(extension.glyph)}</span><button class="switch ${enabled?'on':''}" role="switch" aria-checked="${enabled}" aria-label="切换 ${extension.id}" data-extension="${extension.id}" ${!downloaded||!installed||sessionConfigBusy()?'disabled':''} title="${!installed?'先下载并安装后才能启用':'执行节点启用状态 · 模拟'}"></button></div><h3>${extension.id}</h3><p>${escapeHTML(extension.description)}</p>${resourceSteps(downloaded,installed,enabled)}<div class="extension-card-footer"><span>${pending?'下载中…':enabled?'已启用':installed?'已安装，未启用':downloaded?'已下载，待安装':'未下载'}</span><span>这台电脑 · 模拟</span></div><div class="resource-actions">${!installed?`<button class="button ${downloaded?'':'primary'}" data-resource-action="download" data-resource-kind="extensions" data-resource-id="${extension.id}" ${downloaded||pending?'disabled':''}>${pending?'下载中…':downloaded?'已下载':'下载到执行节点'}</button><button class="button ${downloaded?'primary':''}" data-resource-action="install" data-resource-kind="extensions" data-resource-id="${extension.id}" ${!downloaded||pending?'disabled':''}>安装到执行节点</button>`:'<span class="resource-ready">已下载 · 已安装</span>'}</div></article>`;
 }).join('')}</div>`;
}
function skillAcquisitionState(skill){
 if(skill.packageId){const ready=resourceSkillReady(skill);return ready?'所属 Package 已下载并安装':'随 Package 下载并安装';}
 const store=resourceStore.skills;return !store.downloaded.has(skill.id)?'未下载':!store.installed.has(skill.id)?'已下载，待安装':skill.status==='invalid'?'已安装 · 校验未通过':skill.status==='shadowed'?'已安装 · 同名来源被覆盖':resourceSkillEnabled(skill)&&!packageSession().disabledSkills.includes(skill.id)?'已安装 · 本会话启用':'已安装，未启用';
}
function skillResourceActions(skill){
 if(skill.packageId)return `<button class="button" data-resource-action="package" data-resource-id="${skill.id}">管理所属 Package</button>`;
 const store=resourceStore.skills,downloaded=store.downloaded.has(skill.id),installed=store.installed.has(skill.id),pending=resourceStore.pending.has('skills:'+skill.id);
 return !installed?`<button class="button ${downloaded?'':'primary'}" data-resource-action="download" data-resource-kind="skills" data-resource-id="${skill.id}" ${downloaded||pending?'disabled':''}>${pending?'下载中…':downloaded?'已下载':'下载到执行节点'}</button><button class="button ${downloaded?'primary':''}" data-resource-action="install" data-resource-kind="skills" data-resource-id="${skill.id}" ${!downloaded||pending?'disabled':''}>安装到执行节点</button>`:skill.status==='available'?`<button class="button" data-resource-action="skill-enable" data-resource-id="${skill.id}" ${sessionConfigBusy()?'disabled':''}>${resourceSkillEnabled(skill)&&!packageSession().disabledSkills.includes(skill.id)?'停用本会话':'启用本会话'}</button>`:'';
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button[data-resource-action]');if(!button||button.disabled)return;const {resourceAction:action,resourceKind:kind,resourceId:id}=button.dataset;
 if(action==='download')downloadNodeResource(kind,id);
 if(action==='install')installNodeResource(kind,id);
 if(action==='skill-enable'){
  const skill=skillById(id);if(!skill||!resourceSkillReady(skill)||skill.status!=='available'||sessionConfigBusy())return;
  const bindings=standaloneSkillBindings(),config=packageSession(),enabled=resourceSkillEnabled(skill)&&!config.disabledSkills.includes(id);
  if(enabled)bindings.delete(id);else{if(skill.personalResourceId)skillSamples.filter(item=>item.personalResourceId===skill.personalResourceId&&item.id!==id).forEach(item=>bindings.delete(item.id));bindings.add(id);config.disabledSkills=config.disabledSkills.filter(value=>value!==id);}config.revision++;
  skillSession().draftSkills=skillSession().draftSkills.filter(value=>sessionSkillAvailable(skillById(value)));resourceRefresh();
 }
 if(action==='package'){
  const skill=skillById(id),version=skill.packageVersion||packageSession().bindings[skill.packageId]||packageCatalog.find(p=>p.id===skill.packageId)?.version;
  packageDetail(skill.packageId+'@'+version);
 }
});
