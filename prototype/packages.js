/* Session resources and inert, portable JSON design packages. No node install/network I/O. */
const packageTools=[
 ['read','读取文件','项目内只读'],['grep','检索文件','项目内只读'],['edit','编辑文件','工作区写入'],['powershell','执行命令','按需确认'],['ask_user','向用户提问','会话交互'],['todo','管理待办','会话内状态'],['webfetch','获取网页','外部网络'],['docs.search','文档检索','MCP 授权连接'],['delegate_task','委派子任务','继承受限能力'],['browser','操作浏览器','当前节点未提供']
].map(([id,name,scope])=>({id,name,scope,available:id!=='browser'}));
const packageCatalog=[
 {id:'rpi.rust-workbench',name:'Rust Workbench',version:'1.2.0',author:'RPI Design',source:'内置样例',description:'Rust 编程工作台：审阅技能、基础工具和编码约定。',skills:['global-responsive','global-review','package-notes'],tools:['read','grep','edit','powershell','ask_user','todo'],prompts:[{id:'coding',name:'编码约定',kind:'instruction',content:'优先阅读现有代码，保持修改聚焦，说明实际验证结果。'},{id:'review',name:'审阅变更',kind:'template',content:'请审阅当前变更，按严重程度列出可定位的问题，并说明验证依据。'}]},
 {id:'rpi.research',name:'Research Kit',version:'0.4.0',author:'RPI Design',source:'内置样例',description:'网页、文档和子任务能力，可独立在会话中启用。',skills:[],tools:['webfetch','docs.search','delegate_task','browser'],prompts:[{id:'research',name:'资料检索',kind:'template',content:'请检索并比较相关资料，给出可核对的来源和结论。'}]},
 {id:'rpi.mobile-review',name:'Mobile Review',version:'0.3.0',author:'RPI Design',source:'目录样例',description:'可下载的手机界面审阅包，组合技能、只读工具和审阅提示词。',skills:['mobile-audit'],tools:['read','grep'],prompts:[{id:'mobile',name:'手机体验审阅',kind:'template',content:'请检查手机布局、触控目标、滚动和软键盘遮挡，先报告问题，不修改文件。'}]},
];
packageCatalog.push({...structuredClone(packageCatalog[0]),version:'1.3.0',source:'目录样例',description:'新版编码约定样例；安装后由会话明确切换版本。'});
const packageStore={installed:new Set(['rpi.rust-workbench@1.2.0','rpi.research@0.4.0']),downloaded:new Set(),view:'packages',importOwner:null};
const mobileSkill={id:'mobile-audit',name:'mobile-audit',scope:'package',source:'Mobile Review',packageId:'rpi.mobile-review',path:'<package>/skills/mobile-audit/SKILL.md',status:'available',description:'审阅手机布局、触控目标与键盘遮挡。',trigger:'需要评估手机界面时。',content:'---\nname: mobile-audit\ndescription: Review mobile interaction and layout.\n---\n\n# Mobile audit\n\nInspect touch targets, scrolling and keyboard overlap. Report findings before editing.',refs:[],auto:true};
for(const id of packageCatalog[0].skills){const s=skillById(id);s.packageId=packageCatalog[0].id;}
function packageKey(p){return p.id+'@'+p.version;}
function packageByKey(key){return packageCatalog.find(p=>packageKey(p)===key);}
function packageSkill(p,id){return skillById(id)||p.importedSkills?.find(s=>s.id===id)||(id==='mobile-audit'?mobileSkill:null);}
function packageSession(){
 const s=runtimeSession();
 s.resources||={bindings:{'rpi.rust-workbench':'1.2.0',...(state.session==='dark'?{'rpi.research':'0.4.0'}:{})},disabledSkills:[],disabledTools:[],disabledPrompts:[],instructions:'',revision:1};
 return s.resources;
}
function boundPackages(){return Object.entries(packageSession().bindings).map(([id,v])=>packageByKey(id+'@'+v)).filter(Boolean);}
function sessionConfigBusy(){return state.running||hasPendingAsk();}
function packageActive(id){return !!packageSession().bindings[id];}
function sessionSkillAvailable(skill){return !!skill&&skill.status==='available'&&(!skill.packageId||(packageActive(skill.packageId)&&(!skill.packageVersion||skill.packageVersion===packageSession().bindings[skill.packageId])))&&!packageSession().disabledSkills.includes(skill.id);}
function sessionSkills(){return skillSamples.filter(sessionSkillAvailable);}
function sessionTools(){const ids=new Set(boundPackages().flatMap(p=>p.tools));return packageTools.filter(t=>ids.has(t.id));}
function sessionPrompts(){return boundPackages().flatMap(p=>p.prompts.map(prompt=>({...prompt,key:p.id+':'+prompt.id,packageName:p.name,packageId:p.id,version:p.version})));}
function renderSessionResources(){
 const el=$('#session-resources');if(!el)return;const config=packageSession(),packages=boundPackages(),tools=sessionTools().filter(t=>t.available&&!config.disabledTools.includes(t.id)),prompts=sessionPrompts().filter(p=>!config.disabledPrompts.includes(p.key));
 el.innerHTML=`<div class="session-package-line"><span class="session-resource-label">会话配置 <small>r${config.revision}</small></span><div class="session-package-chips">${packages.length?packages.map(p=>`<button data-package="detail" data-key="${packageKey(p)}" title="${escapeHTML(p.name)} · ${escapeHTML(p.source)}">${icon('blocks')}<span>${escapeHTML(p.name)}</span><small>${p.version}</small></button>`).join(''):'<span class="text-muted">仅使用项目资源</span>'}</div><button class="session-package-manage" data-package="session" data-view="packages">管理 Package ${icon('chevron-right')}</button></div><div class="session-resource-actions"><button data-package="session" data-view="skills">${icon('checklist')}Skills <b>${sessionSkills().length}</b></button><button data-package="session" data-view="tools">${icon('blocks')}Tools <b>${tools.length}</b></button><button data-package="session" data-view="prompts">${icon('file')}提示词 <b>${prompts.length+(config.instructions.trim()?1:0)}</b></button><span>本会话 · 配置用于后续请求</span></div>`;
}
function packageActions(p){
 const key=packageKey(p),installed=packageStore.installed.has(key),active=packageSession().bindings[p.id]===p.version;
 return `<button class="button" data-package="download" data-key="${key}">下载资源包</button>${!installed?`<button class="button" data-package="install" data-key="${key}" ${!packageStore.downloaded.has(key)?'disabled':''}>模拟安装到节点</button>`:`<button class="button ${active?'':'primary'}" data-package="bind" data-key="${key}" ${sessionConfigBusy()?'disabled':''}>${active?'停用本会话':packageSession().bindings[p.id]?'切换到 '+p.version:'用于本会话'}</button>`}`;
}
function packageCard(p){
 const key=packageKey(p),installed=packageStore.installed.has(key),active=packageSession().bindings[p.id]===p.version;
 return `<article class="package-card" data-package-key="${key}"><div class="package-card-title"><span class="extension-icon">${icon('blocks')}</span><div><h3>${escapeHTML(p.name)}</h3><code>${escapeHTML(p.id)} <span>@${p.version}</span></code></div><span class="package-badge ${active?'active':''}">${active?'本会话启用':installed?'已安装 · 模拟':packageStore.downloaded.has(key)?'已下载':'可下载'}</span></div><p>${escapeHTML(p.description)}</p><div class="package-contents"><span>${p.skills.length} Skills</span><span>${p.tools.length} Tools</span><span>${p.prompts.length} 提示词</span></div><small>${escapeHTML(p.author)} · ${escapeHTML(p.source)}</small><div class="package-card-actions"><button class="button" data-package="detail" data-key="${key}">查看内容</button>${packageActions(p)}</div></article>`;
}
function resourceOwner(skill){return skill.packageId?`${packageCatalog.find(p=>p.id===skill.packageId)?.name||skill.packageId} · ${packageSession().bindings[skill.packageId]||'未启用'}`:skill.source+'资源';}
function resourceCheckbox(action,id,checked,disabled=false){return `<input type="checkbox" data-package="${action}" data-id="${escapeHTML(id)}" ${checked?'checked':''} ${disabled?'disabled':''}>`;}
function sessionResources(view='packages'){
 packageStore.view=view;const config=packageSession(),busy=sessionConfigBusy(),heading=state.sessions.find(s=>s.id===state.session)?.title||'当前会话';let body='';
 if(view==='packages')body=`<p>选择用于此会话的具名资源包。同一资源包同时只绑定一个版本。</p><div class="package-stack">${packageCatalog.filter(p=>packageStore.installed.has(packageKey(p))).map(packageCard).join('')}</div><button class="button package-browse" data-package="library">浏览与导入 Package ${icon('chevron-right')}</button>`;
 if(view==='skills')body=`<p>可用目录与全文载入分开显示；指定单次消息的快捷入口仍保留在输入框。</p><div class="session-resource-list">${skillSamples.filter(s=>s.status==='available'&&(!s.packageId||(packageActive(s.packageId)&&(!s.packageVersion||s.packageVersion===config.bindings[s.packageId])))).map(skill=>`<div class="session-resource-row"><label>${resourceCheckbox('skill-toggle',skill.id,!config.disabledSkills.includes(skill.id),busy)}<span><strong>${escapeHTML(skill.name)}</strong><small>${escapeHTML(resourceOwner(skill))} · ${skillStatus(skill)}</small></span></label><button class="small-link" data-capability="skill-detail" data-id="${skill.id}">说明</button></div>`).join('')}</div><button class="button package-browse" data-package="skill-library">浏览完整技能库</button>`;
 if(view==='tools')body=`<p>这里设置本会话的工具目录；调用参数、执行进度和输出继续出现在对话中。</p><div class="session-resource-list">${sessionTools().map(tool=>`<div class="session-resource-row"><label>${resourceCheckbox('tool-toggle',tool.id,tool.available&&!config.disabledTools.includes(tool.id),busy||!tool.available)}<span><strong>${escapeHTML(tool.id)} <em>${tool.name}</em></strong><small>${tool.scope} · ${boundPackages().filter(p=>p.tools.includes(tool.id)).map(p=>escapeHTML(p.name)).join(' / ')}</small></span></label><span class="package-badge">${!tool.available?'节点未提供':config.disabledTools.includes(tool.id)?'未启用':'目录已启用'}</span></div>`).join('')||'<div class="empty-compact">当前没有 Package 提供工具。</div>'}</div><p class="modal-help">工具启用不替代执行权限。原型仅配置样例目录，不会更改真实宿主。</p>`;
 if(view==='prompts')body=`<p>会话指令用于后续请求；提示词模板只在明确套用后插入草稿。</p><div class="session-resource-list">${sessionPrompts().map(prompt=>`<div class="session-prompt"><div class="session-resource-row"><label>${resourceCheckbox('prompt-toggle',prompt.key,!config.disabledPrompts.includes(prompt.key),busy)}<span><strong>${escapeHTML(prompt.name)} <em>${prompt.kind==='instruction'?'会话指令':'消息模板'}</em></strong><small>${escapeHTML(prompt.packageName)} · ${prompt.version}</small></span></label>${prompt.kind==='template'?`<button class="small-link" data-package="use-prompt" data-id="${escapeHTML(prompt.key)}" ${config.disabledPrompts.includes(prompt.key)?'disabled':''}>套用到草稿</button>`:''}</div><pre>${escapeHTML(prompt.content)}</pre></div>`).join('')}</div><div class="field session-instructions"><label for="session-instructions">本会话补充指令</label><textarea id="session-instructions" class="modal-textarea" rows="3" maxlength="4000" ${busy?'disabled':''} placeholder="例如：回复用中文；修改前先说明影响范围。">${escapeHTML(config.instructions)}</textarea></div><button class="button" data-package="save-instructions" ${busy?'disabled':''}>保存会话指令</button>`;
 runtimeModal('会话资源',`<div class="session-resource-caption">${escapeHTML(heading)} · 这台电脑 · 演示</div><div class="segmented resource-segments">${[['packages','Packages'],['skills','Skills'],['tools','Tools'],['prompts','提示词']].map(([key,label])=>`<button data-package="session" data-view="${key}" class="${view===key?'active':''}">${label}</button>`).join('')}</div>${busy?'<div class="info-banner">本轮正在执行或等待回答，结束后可修改会话配置。</div>':''}${body}`,'',true);
}
function renderPackageLibrary(){
 $('#page-view').innerHTML=`<div class="overline">COMPOSE YOUR SESSION.</div><div class="page-heading"><div><h1>Packages，给会话一组能力</h1><p>具名、版本化的 Skills、工具声明与提示词组合。</p></div><button class="button" data-package="import">${icon('plus')}导入资源包</button></div><div class="package-library-note">资源库属于执行节点；是否启用由当前会话决定。此处下载为可重新导入的 JSON 设计包，安装到节点是模拟操作。</div><div class="package-library-context"><span>当前会话：${escapeHTML(state.sessions.find(s=>s.id===state.session)?.title||'新会话')}</span><button class="small-link" data-package="return">返回对话 ${icon('chevron-right')}</button></div><div class="package-grid">${packageCatalog.map(packageCard).join('')}</div>`;
}
function packageManifest(p){
 return {schemaVersion:1,kind:'rpi-work-design-package',id:p.id,name:p.name,version:p.version,author:p.author,description:p.description,resources:{skills:p.skills.map(id=>{const s=packageSkill(p,id);return {id:s.resourceId||s.id,name:s.name,description:s.description,content:s.content};}),tools:[...p.tools],prompts:structuredClone(p.prompts)}};
}
function packageDetail(key){
 const p=packageByKey(key);if(!p)return;
 runtimeModal(escapeHTML(p.name),`<div class="package-detail-meta"><code>${escapeHTML(p.id)} @${p.version}</code><span>${escapeHTML(p.source)} · ${escapeHTML(p.author)}</span></div><p>${escapeHTML(p.description)}</p><div class="package-detail-content"><h3>Skills</h3>${p.skills.map(id=>`<div>${escapeHTML(packageSkill(p,id).name)}</div>`).join('')||'<small>未包含</small>'}<h3>Tools</h3><div class="package-tool-tags">${p.tools.map(id=>`<code>${escapeHTML(id)}</code>`).join('')}</div><h3>提示词</h3>${p.prompts.map(prompt=>`<details><summary>${escapeHTML(prompt.name)} · ${prompt.kind==='instruction'?'会话指令':'模板'}</summary><pre>${escapeHTML(prompt.content)}</pre></details>`).join('')}</div><p class="modal-help">下载文件包含技能正文、提示词正文和工具声明。原生工具实现需要执行节点提供；JSON 资源包不包含可执行插件。</p><div class="package-detail-actions">${packageActions(p)}</div>`,'',true);
 $('#modal').dataset.packageDetail=key;
}
function refreshPackages(){
 renderSessionResources();renderCapabilitiesUI();
 if(state.page==='packages')renderPackageLibrary();
 if($('#modal').open){if($('#modal').dataset.packageDetail)packageDetail($('#modal').dataset.packageDetail);else sessionResources(packageStore.view);}
}
function downloadPackage(key){
 const p=packageByKey(key);if(!p)return;
 const blob=new Blob([JSON.stringify(packageManifest(p),null,2)+'\n'],{type:'application/json'}),url=URL.createObjectURL(blob),a=document.createElement('a');
 a.href=url;a.download=`${p.id}-${p.version}.rpi-package.json`;document.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);packageStore.downloaded.add(key);refreshPackages();toast('已导出可重新导入的 JSON 资源包；尚未安装或启用');
}
function registerPackageSkills(p){
 for(const id of p.skills){if(skillById(id))continue;const data=p.importedSkills?.find(s=>s.id===id)||mobileSkill;skillSamples.push({...data,scope:'package',source:p.name,packageId:p.id,packageVersion:p.version,path:`<${p.id}>/skills/${data.name}/SKILL.md`,status:'available',trigger:'由此资源包提供，按描述匹配或手动指定。',refs:[],auto:true});}
}
function installDesignPackage(key){const p=packageByKey(key);if(!p||!packageStore.downloaded.has(key))return;packageStore.installed.add(key);registerPackageSkills(p);refreshPackages();toast('已模拟安装到执行节点；请明确选择是否用于本会话');}
function mutateSessionResource(list,id){
 if(sessionConfigBusy()){toast('请在本轮结束后修改会话配置');return false;}
 const s=packageSession();s[list]=s[list].includes(id)?s[list].filter(v=>v!==id):[...s[list],id];s.revision++;
 const drafts=skillSession().draftSkills;skillSession().draftSkills=drafts.filter(id=>sessionSkillAvailable(skillById(id)));return true;
}
function bindDesignPackage(key){
 if(sessionConfigBusy()){toast('请在本轮结束后修改会话配置');return;}
 const p=packageByKey(key);if(!p||!packageStore.installed.has(key))return;const s=packageSession();
 if(s.bindings[p.id]===p.version)delete s.bindings[p.id];else s.bindings[p.id]=p.version;
 s.revision++;skillSession().draftSkills=skillSession().draftSkills.filter(id=>sessionSkillAvailable(skillById(id)));refreshPackages();toast('已更新本会话后续请求的配置；历史记录保留');
}
function captureSessionResources(){const s=packageSession();return {revision:s.revision,packages:boundPackages().map(p=>({id:p.id,name:p.name,version:p.version})),skills:sessionSkills().map(s=>s.id),tools:sessionTools().filter(t=>t.available&&!s.disabledTools.includes(t.id)).map(t=>t.id),instructions:[...sessionPrompts().filter(p=>p.kind==='instruction'&&!s.disabledPrompts.includes(p.key)).map(p=>({key:p.key,content:p.content})),...(s.instructions.trim()?[{key:'session:custom',content:s.instructions.trim()}]:[])]};}
function renderResourceSnapshot(snapshot){return snapshot?`<details class="message-resource-snapshot"><summary>会话配置 r${snapshot.revision} · ${snapshot.packages.length} Package · 本轮快照</summary><div>${snapshot.packages.map(p=>`<code>${escapeHTML(p.name)} @${p.version}</code>`).join('')}<span>${snapshot.skills.length} Skills · ${snapshot.tools.length} Tools · ${snapshot.instructions.length} 会话指令</span></div></details>`:'';}
function validateDesignPackage(data){
 const text=(s,max=200)=>typeof s==='string'&&s.trim().length>0&&s.length<=max,token=s=>text(s,90)&&!['__proto__','prototype','constructor'].includes(s)&&/^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(s);
 if(data?.schemaVersion!==1||data.kind!=='rpi-work-design-package'||!token(data.id)||!text(data.name,80)||!/^\d+\.\d+\.\d+$/.test(data.version)||!text(data.author,80)||!text(data.description,1000))throw Error('资源包格式不正确：请使用 schemaVersion 1 的 JSON 设计包。');
 const r=data.resources;if(!r||!['skills','tools','prompts'].every(k=>Array.isArray(r[k])&&r[k].length<=20))throw Error('资源清单缺失或超出 20 项限制。');
 if(r.tools.some(id=>!packageTools.some(t=>t.id===id)))throw Error('包中有未知工具声明；当前原型只支持目录中已有的工具。');
 if(r.skills.some(s=>!token(s.id)||!token(s.name)||!text(s.description,1000)||!text(s.content,40000))||r.prompts.some(p=>!token(p.id)||!text(p.name,80)||!['instruction','template'].includes(p.kind)||!text(p.content,40000)))throw Error('技能或提示词内容格式不正确。');
 if(new Set(r.skills.map(s=>s.id)).size!==r.skills.length||new Set(r.prompts.map(p=>p.id)).size!==r.prompts.length)throw Error('同一包内的资源 ID 不能重复。');
 const existing=packageByKey(data.id+'@'+data.version);
 if(existing){
  const candidate={schemaVersion:1,kind:'rpi-work-design-package',id:data.id,name:data.name,version:data.version,author:data.author,description:data.description,resources:{skills:r.skills.map(s=>({id:s.id,name:s.name,description:s.description,content:s.content})),tools:[...new Set(r.tools)],prompts:r.prompts.map(p=>({id:p.id,name:p.name,kind:p.kind,content:p.content}))}};
  if(JSON.stringify(candidate)!==JSON.stringify(packageManifest(existing)))throw Error('同一 ID 和版本的内容不一致，请使用新版本号。');
  return existing;
 }
 const mapped=r.skills.map(s=>({id:`${data.id}@${data.version}:${s.id}`,resourceId:s.id,name:s.name,description:s.description,content:s.content}));
 return {id:data.id,name:data.name,version:data.version,author:data.author,description:data.description,source:'本地导入',skills:mapped.map(s=>s.id),importedSkills:mapped,tools:[...new Set(r.tools)],prompts:r.prompts.map(p=>({id:p.id,name:p.name,kind:p.kind,content:p.content}))};
}
async function importDesignPackage(file){
 const owner=packageStore.importOwner;try{
  if(!file)return;if(file.size>256*1024)throw Error('设计包大小不能超过 256 KB。');
  const p=validateDesignPackage(JSON.parse(await file.text()));if(!packageCatalog.includes(p))packageCatalog.push(p);packageStore.downloaded.add(packageKey(p));
  if(state.page==='packages')renderPackageLibrary();
  if(owner!==state.session){toast('资源包已导入到资源库；没有修改任何会话');return;}
  if($('#modal').open)$('#modal').close();navigate('packages');toast('资源包已导入，尚未模拟安装或用于会话');
 }catch(e){toast(e instanceof SyntaxError?'无法解析 JSON 资源包。':e.message);}finally{$('#package-input').value='';}
}
document.addEventListener('click',event=>{
 const b=event.target.closest('button[data-package]');if(!b||b.disabled)return;const action=b.dataset.package,key=b.dataset.key,id=b.dataset.id;
 if(action==='session'){delete $('#modal').dataset.packageDetail;sessionResources(b.dataset.view);}
 if(action==='detail')packageDetail(key);
 if(action==='download')downloadPackage(key);
 if(action==='install')installDesignPackage(key);
 if(action==='bind')bindDesignPackage(key);
 if(action==='library'){if($('#modal').open)$('#modal').close();navigate('packages');}
 if(action==='return'){navigate('workspace');renderSessionResources();}
 if(action==='skill-library'){if($('#modal').open)$('#modal').close();navigate('skills');}
 if(action==='import'){packageStore.importOwner=state.session;$('#package-input').click();}
 if(action==='save-instructions'&&!sessionConfigBusy()){const s=packageSession();s.instructions=$('#session-instructions').value.trim();s.revision++;renderSessionResources();toast('已保存本会话指令，用于后续请求');}
 if(action==='use-prompt'){
  const p=sessionPrompts().find(p=>p.key===id);if(!p||packageSession().disabledPrompts.includes(id))return;
  $('#prompt').value+=($('#prompt').value?'\n\n':'')+p.content;$('#modal').close();navigate('workspace');$('#prompt').focus();toast('模板已插入草稿，尚未发送');
 }
});
document.addEventListener('change',event=>{
 if(event.target.id==='package-input'){importDesignPackage(event.target.files[0]);return;}
 const target=event.target;if(!target.matches('input[data-package]'))return;
 const list={'skill-toggle':'disabledSkills','tool-toggle':'disabledTools','prompt-toggle':'disabledPrompts'}[target.dataset.package];if(list){mutateSessionResource(list,target.dataset.id);refreshPackages();}
});
