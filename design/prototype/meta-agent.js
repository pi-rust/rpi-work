/* Personal capability authoring design. Source edits, structural checks, test snapshots and releases stay in memory. */
const metaAgent={projects:[{id:'rpi-work',name:'rpi-work',type:'project',project:'rpi-work',path:'D:/Projects/rpi-work',lastSession:'dark'},{id:'pi-rust',name:'pi-rust',type:'project',project:'pi-rust',path:'D:/Projects/pi-rust',lastSession:'rpc'},{id:'rpi-package',name:'rpi-package',type:'project',project:'rpi-package',path:'D:/Projects/rpi-package',lastSession:'plugin'}],resources:[],releases:[],selected:null,tab:'drafts',editor:null,publish:null};
const metaKinds={skill:'Skill',package:'Package',extension:'扩展'};
function activeProjectProfile(){return metaAgent.projects.find(p=>p.project===state.project)||metaAgent.projects[0];}
function metaResource(id){return metaAgent.resources.find(r=>r.id===id&&r.projectId===activeProjectProfile().id);}
function metaSession(){return state.sessions.find(s=>s.id===state.session);}
function projectOptions(){return metaAgent.projects.map(p=>`<option value="${escapeHTML(p.project)}">${escapeHTML(p.name)}</option>`).join('');}
function activateSessionProject(session){
 const current=activeProjectProfile();current.lastSession=state.session;if(current.type==='capability')current.metaTab=metaAgent.tab;
 const next=metaAgent.projects.find(p=>p.project===session.project);if(next){next.lastSession=session.id;if(session.metaResource)next.selected=session.metaResource;metaAgent.tab=next.metaTab||'drafts';}
}
function renderProjectNavigation(){
 const nav=$('#meta-agent-nav'),container=$('#added-projects');if(!nav||!container)return;const profile=activeProjectProfile(),sessions=$('#session-list');
 if(container.contains(sessions))container.before(sessions);
 container.innerHTML=metaAgent.projects.filter(p=>!['rpi-work','pi-rust','rpi-package'].includes(p.id)).map(p=>`<button class="project-row added-project ${p.project===state.project?'selected':''}" data-project="${escapeHTML(p.project)}"><span class="project-initial">${p.type==='capability'?icon('sparkles'):'P'}</span><span>${escapeHTML(p.name)}</span><small>${p.type==='capability'?'元Agent':'项目'}</small></button>`).join('');
 $$('.sidebar [data-project]').forEach(row=>{const item=metaAgent.projects.find(p=>p.project===row.dataset.project);if(item){const label=$(':scope > span:nth-child(2)',row);if(label)label.textContent=item.name;row.title=item.path;}});
 const row=$$('.sidebar [data-project]').find(el=>el.dataset.project===state.project);if(row)row.after(sessions);
 nav.hidden=profile.type!=='capability';
 const footer=$('.composer-foot>span:first-child');
 if(footer){if(profile.type==='capability'){footer.dataset.metaFooter='true';footer.textContent=metaSession()?.metaTest?`元Agent · 测试快照 r${metaSession().metaTest.revision}`:'元Agent · 开发会话';}else if(footer.dataset.metaFooter){delete footer.dataset.metaFooter;footer.innerHTML='<i class="status-dot"></i>在这台电脑运行 <span class="foot-dot">·</span> Dioxus + rpi';}}
}
function ensureAuthoringPackage(){
 if(packageByKey('meta.agent-authoring@0.1.0'))return;
 packageCatalog.push({id:'meta.agent-authoring',name:'元Agent 开发助手',version:'0.1.0',author:'个人',source:'预置开发包 · 示例',description:'能力源码、结构校验、测试快照和发布约定。',skills:[],tools:['read','grep','edit','ask_user','todo'],prompts:[{id:'authoring',name:'能力开发约定',kind:'instruction',content:'围绕当前能力源码工作。区分结构校验、测试快照和真实运行；发布新版本后再由使用项目明确获取和启用。'}]});
 packageStore.downloaded.add('meta.agent-authoring@0.1.0');packageStore.installed.add('meta.agent-authoring@0.1.0');
}
function configureMetaSession(session){
 ensureAuthoringPackage();runtimeSession(session.id).resources={bindings:{'meta.agent-authoring':'0.1.0'},disabledSkills:[],disabledTools:[],disabledPrompts:[],instructions:'个人能力开发；保留源码、测试快照和发布版本的关联。',revision:1};
}
function metaTemplate(kind,id,name,version){
 const slug=id.split('.').at(-1),skill=`---\nname: ${slug}\ndescription: Review an explicit scope and report evidence.\n---\n\n# ${name}\n\n先确认任务范围，再检查项目资料。\n输出包含结论、文件依据和待确认项。\n`;
 if(kind==='skill')return {[`skills/${slug}/SKILL.md`]:skill,[`skills/${slug}/examples.md`]:'# 试用问题\n\n请审阅一个包含错误处理变更的任务，给出可定位的依据。\n'};
 if(kind==='extension')return {'extension.design.json':JSON.stringify({kind:'rpi-work-extension-design',id,name,version,entry:'src/lib.rs',description:'检查项目相对路径的纯函数示例；尚未适配原生扩展宿主。'},null,2),'src/lib.rs':'// 元Agent 扩展源码草稿；尚未注册到原生宿主。\npub fn is_relative_project_path(path: &str) -> bool {\n    !path.is_empty() && !path.starts_with(\'/\') && !path.contains("..")\n}\n','tests/cases.md':'# 测试场景\n\n- 接受 src/lib.rs\n- 拒绝 ../secrets\n- 原生构建与运行尚未执行\n'};
 return {'package.json':JSON.stringify({schemaVersion:1,kind:'rpi-work-design-package',id,name,version,author:'个人',description:'组合审阅技能、只读工具与任务提示词。',resources:{skills:[{id:slug,name:slug,description:'审阅明确范围并提供文件依据。',content:skill}],tools:['read','grep'],prompts:[{id:'review',name:'审阅任务',kind:'template',content:'请确认任务范围，再审阅相关文件，给出结论与依据。'}]}},null,2),'README.md':`# ${name}\n\n个人资源包草稿。\n\n仅使用只读工具，输出文件依据。\n`,'tests/cases.md':'# 试用场景\n\n- 输出包含文件依据\n- 不声明未提供的工具\n- 模型行为仍需在真实宿主验证\n'};
}
function createMetaResource(profile,kind,name,id,version='0.1.0'){
 const sources=metaTemplate(kind,id,name,version),resource={uid:'resource-'+crypto.randomUUID(),projectId:profile.id,kind,name,id,version,sources,base:structuredClone(sources),revision:1,validation:null,tests:[],releases:[],devSession:null};
 metaAgent.resources.push(resource);profile.selected=resource.uid;syncMetaFiles(profile);return resource;
}
function selectedMetaResource(){const w=activeProjectProfile();return metaAgent.resources.find(r=>r.uid===w.selected&&r.projectId===w.id)||metaAgent.resources.find(r=>r.projectId===w.id);}
function ensureMetaProject(profile){
 if(metaAgent.resources.some(r=>r.projectId===profile.id))return;
 const prefix=profile.id==='meta-personal'?'personal':'personal.'+profile.id.slice(-6);
 createMetaResource(profile,'package','API 审阅包',prefix+'.api-review');
 createMetaResource(profile,'skill','变更说明 Skill',prefix+'.change-notes');
 createMetaResource(profile,'extension','项目路径检查扩展',prefix+'.path-check');
 profile.selected=metaAgent.resources.find(r=>r.projectId===profile.id).uid;
}
function metaDevSession(resource){
 let session=state.sessions.find(s=>s.id===resource.devSession);if(session)return session;
 const profile=metaAgent.projects.find(w=>w.id===resource.projectId);session={id:'meta-dev-'+crypto.randomUUID(),title:'开发 · '+resource.name,project:profile.project,projectId:profile.id,metaResource:resource.uid};
 state.sessions.unshift(session);resource.devSession=session.id;configureMetaSession(session);
 state.messages[session.id]=[{role:'user',text:'为我开发 '+resource.name+'，保留测试与发布记录。'},{role:'assistant',text:'源码草稿已准备。可以从能力项目查看和编辑文件，完成结构校验后加载到独立测试会话，再发布到个人资源库。当前为设计原型，尚未连接真实模型、构建或扩展宿主。'}];return session;
}
function selectProject(project){
 if(state.running&&project!==state.project){toast('请先结束当前生成，再切换项目');return;}
 const profile=metaAgent.projects.find(p=>p.project===project);if(!profile)return;
 if(profile.type==='capability'){
  ensureMetaProject(profile);const resource=metaAgent.resources.find(r=>r.projectId===profile.id&&r.uid===profile.selected)||metaAgent.resources.find(r=>r.projectId===profile.id);
  const session=state.sessions.find(s=>s.id===profile.lastSession)||metaDevSession(resource);if($('#modal').open)$('#modal').close();selectSession(session.id);navigate('meta-agent');
 }else{
  let session=state.sessions.find(s=>s.id===profile.lastSession)||state.sessions.find(s=>s.project===project);
  if(!session){session={id:'project-'+crypto.randomUUID(),title:'开始新的项目任务',project,projectId:profile.id};state.sessions.push(session);}
  if($('#modal').open)$('#modal').close();selectSession(session.id);
 }
}
function metaState(resource){return resource.releases.some(v=>v.revision===resource.revision)?'已发布':resource.tests.some(test=>test.revision===resource.revision&&test.passed)?'可测试':'草稿';}
function metaCanPublish(resource){return !!resource.validation?.ok&&resource.validation.revision===resource.revision&&resource.tests.some(t=>t.revision===resource.revision&&t.passed)&&!resource.releases.some(v=>v.version===resource.version);}
function renderMetaAgent(){
 const w=activeProjectProfile();if(w.type!=='capability'){navigate('workspace');return;}
 const resources=metaAgent.resources.filter(r=>r.projectId===w.id),selected=selectedMetaResource();
 $('#page-view').innerHTML=`<div class="page-heading meta-heading"><div><div class="overline">META AGENT / PERSONAL</div><h1>元Agent</h1><p>把自己的工作方式，写成可复用的能力。</p></div><button class="button primary" data-meta-action="new-resource">${icon('plus')}新建能力</button></div><div class="meta-overview"><span>${icon('blocks')}<strong>${resources.length}</strong> 个能力项目</span><span>${icon('checklist')}<strong>${resources.reduce((n,r)=>n+r.tests.length,0)}</strong> 次测试快照</span><span>${icon('folder')}<strong>${metaAgent.releases.length}</strong> 个个人版本</span><small>源码与运行均为原型演示</small></div><div class="meta-sections" role="group" aria-label="元Agent 内容"><button data-meta-view="drafts" aria-pressed="${metaAgent.tab==='drafts'}">开发中</button><button data-meta-view="library" aria-pressed="${metaAgent.tab==='library'}">个人资源库 <small>${metaAgent.releases.length}</small></button></div>${metaAgent.tab==='library'?renderMetaLibrary():`<div class="meta-project-layout"><aside class="meta-resource-list" aria-label="能力项目">${resources.map(r=>`<button data-meta-select="${r.uid}" ${selected?.uid===r.uid?'aria-current="true"':''}><span class="meta-kind">${metaKinds[r.kind]}</span><strong>${escapeHTML(r.name)}</strong><small>${escapeHTML(r.id)}</small><span>${metaState(r)} · ${r.version} · r${r.revision}</span></button>`).join('')}</aside><section class="meta-project-detail">${selected?renderMetaProject(selected):'<div class="empty-compact">新建第一个能力项目，开始编写自己的 Agent 能力。</div>'}</section></div>`}`;
}
function renderMetaProject(r){
 const valid=r.validation?.revision===r.revision&&r.validation.ok,test=r.tests.findLast(t=>t.revision===r.revision),published=r.releases.some(v=>v.revision===r.revision);
 return `<header class="meta-project-title"><div><span class="meta-kind">${metaKinds[r.kind]} · ${metaState(r)}</span><h2>${escapeHTML(r.name)}</h2><code>${escapeHTML(r.id)} @${r.version} · r${r.revision}</code></div><button class="button" data-meta-action="develop">${icon('message')}开发对话</button></header><div class="meta-lifecycle"><span class="done">1 编写源码</span><i>→</i><span class="${valid?'done':''}">2 结构校验</span><i>→</i><span class="${test?.passed?'done':''}">3 测试快照</span><i>→</i><span class="${published?'done':''}">4 发布版本</span></div><section class="meta-source-card"><div><strong>源码与清单</strong><span>${Object.keys(r.sources).length} 个文件 · 在项目文件中可预览</span></div><button class="button" data-meta-action="edit">${icon('file')}编写源码</button><div class="meta-source-paths">${Object.keys(r.sources).map(path=>`<code>${escapeHTML(path)}</code>`).join('')}</div></section><div class="meta-project-actions"><button class="button" data-meta-action="validate">${icon('checklist')}结构校验</button><button class="button ${valid?'primary':''}" data-meta-action="test" ${!valid||state.running?'disabled':''}>${icon('play')}创建测试会话</button><button class="button ${metaCanPublish(r)?'primary':''}" data-meta-action="publish" ${!metaCanPublish(r)?'disabled':''}>发布个人版本</button>${r.releases.some(v=>v.version===r.version)?'<button class="button" data-meta-action="iterate">继续迭代</button>':''}</div><p class="meta-project-help">测试使用当前草稿的独立快照；发布后，其他项目仍需下载、安装和启用。结构校验不代表真实模型或原生扩展已验证。</p>${r.validation?.revision===r.revision?`<div class="meta-checks">${r.validation.checks.map(c=>`<div class="${c.ok?'passed':'failed'}">${icon(c.ok?'check':'x')}<span>${escapeHTML(c.name)}</span><small>${c.ok?'通过':'未通过'}</small></div>`).join('')}</div>`:''}<section class="meta-tests"><h3>测试与版本记录</h3>${r.tests.length?r.tests.slice().reverse().map(t=>`<button data-meta-open-test="${t.id}"><span>${icon('message')}测试 r${t.revision} · ${t.passed?'结构检查通过':'未通过'}</span><small>${traceDuration(t.duration)} · 独立快照 ${t.revision===r.revision?'':'· 旧草稿'}</small>${icon('chevron-right')}</button>`).join(''):'<p>完成结构校验后，创建测试会话。对话、工具和耗时继续在会话内查看。</p>'}${r.releases.map(v=>`<div class="meta-release-note">${icon('blocks')}已发布 ${v.version} · r${v.revision} · 设计版本</div>`).join('')}</section>`;
}
function renderMetaLibrary(){return `<div class="meta-library-intro">个人发布物供自己的项目复用。已发布版本保留源码与测试快照，发布不会自动启用。</div><div class="meta-release-grid">${metaAgent.releases.map(release=>`<article class="meta-release-card"><span class="meta-kind">${metaKinds[release.kind]} · 个人</span><h2>${escapeHTML(release.name)}</h2><code>${escapeHTML(release.id)} @${release.version}</code><p>来源 ${escapeHTML(release.projectName)} · 草稿 r${release.revision} · 关联测试快照</p><small>已发布设计版本 · 下载和安装由执行节点管理</small><button class="button" data-meta-acquire="${release.key}">前往资源库获取 ${icon('chevron-right')}</button></article>`).join('')||'<div class="empty-compact">还没有发布版本。先完成一个能力项目的校验与测试快照。</div>'}</div>`;}
function newMetaResourceForm(){showModal('新建能力项目',`<div class="field"><label for="meta-resource-kind">能力类型</label><select id="meta-resource-kind"><option value="package">Package · 组合技能、工具与提示词</option><option value="skill">Skill · 操作指导与参考资料</option><option value="extension">扩展 · 工具实现与宿主行为</option></select></div><div class="field"><label for="meta-resource-name">名称</label><input id="meta-resource-name" placeholder="例如：API 审阅包" maxlength="60"></div><div class="field"><label for="meta-resource-id">资源 ID</label><input id="meta-resource-id" value="personal.capability-${crypto.randomUUID().slice(0,6)}" maxlength="80"></div><p class="modal-help">从 0.1.0 草稿开始；本次只创建原型中的源码与会话。</p>`,`<button class="button" data-close-modal>取消</button><button class="button primary" data-meta-action="create-resource">创建能力项目</button>`);}
function metaOpenEditor(){
 const r=selectedMetaResource();metaAgent.editor={uid:r.uid,drafts:structuredClone(r.sources),path:Object.keys(r.sources)[0]};
 showModal('编写能力源码',`<p>${escapeHTML(r.name)} · 草稿 r${r.revision} · 保存后需重新校验和测试</p><div class="meta-editor-layout"><nav class="meta-editor-files" aria-label="能力源码文件">${Object.keys(r.sources).map(path=>`<button data-meta-file="${escapeHTML(path)}">${icon('file')}${escapeHTML(path)}</button>`).join('')}</nav><div class="meta-editor-content"><label id="meta-editor-path" for="meta-source-editor"></label><textarea id="meta-source-editor" spellcheck="false" aria-label="编辑能力源码"></textarea><small>仅编辑内存中的设计源码；不会执行代码或修改真实项目文件。</small></div></div>`,`<button class="button" data-close-modal>取消</button><button class="button primary" data-meta-action="save-source">保存草稿</button>`);$('#modal').classList.add('meta-editor-modal');$('#modal').classList.toggle('meta-phone-editor',$('#app').classList.contains('phone-preview'));renderMetaEditorFile();
}
function renderMetaEditorFile(){const editor=metaAgent.editor;$('#meta-editor-path').textContent=editor.path;$('#meta-source-editor').value=editor.drafts[editor.path];$$('[data-meta-file]').forEach(button=>button.setAttribute('aria-current',button.dataset.metaFile===editor.path?'true':'false'));}
function checkMetaSources(r,sources=r.sources){
 const checks=[{name:'源码文件非空且不超过 200 KiB',ok:Object.values(sources).every(text=>text.trim()&&new TextEncoder().encode(text).length<=200*1024)}];
 if(r.kind==='package'){let manifest;try{manifest=JSON.parse(sources['package.json']);validateDesignPackage(manifest);checks.push({name:'Package 清单、工具声明与资源格式',ok:manifest.id===r.id&&manifest.version===r.version});}catch{checks.push({name:'Package 清单、工具声明与资源格式',ok:false});}}
 else if(r.kind==='skill'){const text=Object.entries(sources).find(([path])=>path.endsWith('/SKILL.md'))?.[1]||'';checks.push({name:'SKILL.md 包含名称、描述和正文',ok:/^---\s*\nname:\s*\S+[\s\S]*\ndescription:\s*\S+[\s\S]*\n---[\s\S]+/.test(text)});}
 else{let manifest;try{manifest=JSON.parse(sources['extension.design.json']);}catch{}checks.push({name:'扩展设计清单与源码入口一致',ok:manifest?.kind==='rpi-work-extension-design'&&manifest.id===r.id&&manifest.version===r.version&&!!sources[manifest.entry]});}
 checks.push({name:'包含试用说明或测试场景',ok:Object.keys(sources).some(path=>/examples|tests/.test(path))});return {checks,ok:checks.every(c=>c.ok)};
}
function metaCreateTest(r){
 const started=performance.now(),result=checkMetaSources(r);if(!result.ok||r.validation?.revision!==r.revision){toast('请先通过当前草稿的结构校验');return;}
 const profile=activeProjectProfile(),snapshot={id:r.id,name:r.name,kind:r.kind,version:r.version,revision:r.revision,sources:structuredClone(r.sources)},id='meta-test-'+crypto.randomUUID(),duration=performance.now()-started;
 const session={id,title:`测试 r${r.revision} · ${r.name}`,project:profile.project,projectId:profile.id,metaResource:r.uid,metaTest:snapshot};state.sessions.unshift(session);configureMetaSession(session);
 state.messages[id]=[{role:'user',text:`加载 ${r.name} @${r.version} 草稿 r${r.revision} 的独立测试快照。`},{role:'assistant',text:`已固定草稿 r${r.revision} 的源码快照。${result.checks.length} 项本地结构检查通过；未调用真实模型、构建扩展或执行行为测试。可以继续输入试用问题，流式回复为本地演示。`}];
 r.tests.push({id,revision:r.revision,passed:true,duration,snapshot});
 traceBucket(id).push({id:'trace-'+id,session:id,name:'草稿快照结构检查',provider:null,source:'local',status:'completed',startUtc:Date.now(),duration,firstText:null,seen:new Set(),metadata:{project:profile.project,resource:r.id,revision:r.revision,verification:'仅本地结构检查'},spans:[{id:'root',parent:null,kind:'context',name:'草稿快照结构检查',start:0,end:duration,status:'completed',input:JSON.stringify({resource:r.id,revision:r.revision}),output:JSON.stringify(result.checks,null,2)}]});
 selectSession(id);$('#prompt').value='请根据此测试快照，先说明应如何验证能力的行为。';
}
function metaPublishForm(r){
 if(!metaCanPublish(r))return;metaAgent.publish={uid:r.uid,revision:r.revision};
 showModal('发布到个人资源库',`<p>发布当前源码和关联测试快照，保留一个独立设计版本。</p><div class="meta-publish-summary"><strong>${escapeHTML(r.name)}</strong><code>${escapeHTML(r.id)} @${r.version}</code><span>草稿 r${r.revision} · ${Object.keys(r.sources).length} 个文件 · 结构检查通过</span></div><p class="modal-help">其他项目仍需下载、安装和启用。原型不执行真实构建、插件安装或网络发布。</p>`,`<button class="button" data-close-modal>取消</button><button class="button primary" data-meta-action="confirm-publish">发布个人版本</button>`);
}
function confirmMetaPublish(){
 const r=metaAgent.resources.find(r=>r.uid===metaAgent.publish?.uid);if(!r||r.projectId!==activeProjectProfile().id||r.revision!==metaAgent.publish.revision||!metaCanPublish(r)){toast('草稿已变化，请重新校验和测试');return;}
 const key=r.kind+':'+r.id+'@'+r.version;if(metaAgent.releases.some(release=>release.key===key)){toast('这个版本已经发布，请继续迭代一个新版本');return;}
 const release={key,id:r.id,kind:r.kind,name:r.name,version:r.version,revision:r.revision,projectName:activeProjectProfile().name,sources:structuredClone(r.sources)};
 if(r.kind==='package'){const pkg=validateDesignPackage(JSON.parse(r.sources['package.json']));pkg.source='个人资源库';if(!packageCatalog.includes(pkg))packageCatalog.push(pkg);release.catalogId=packageKey(pkg);}
 else if(r.kind==='skill'){
  const id=r.id+'@'+r.version;skillSamples.push({id,personalResourceId:r.id,version:r.version,name:r.name,scope:'global',source:'个人资源库 · '+r.version,path:`<personal>/${r.id}/${r.version}/SKILL.md`,status:'available',description:'元Agent 发布的个人 Skill 设计版本。',trigger:'下载、安装并在会话中启用后手动指定。',content:Object.entries(r.sources).find(([path])=>path.endsWith('/SKILL.md'))[1],refs:[],auto:false});release.catalogId=id;
 }else{const id=r.id+'@'+r.version;extensionCatalog.push({id,personalResourceId:r.id,version:r.version,glyph:'terminal',description:'元Agent 发布的扩展设计版本；尚未适配原生宿主。'});release.catalogId=id;}
 metaAgent.releases.push(release);r.releases.push({version:r.version,revision:r.revision,key});metaAgent.tab='library';activeProjectProfile().metaTab='library';$('#modal').close();navigate('meta-agent');toast('已发布个人设计版本，尚未下载、安装或启用');
}
function metaEntries(r,sources){return Object.entries(sources).map(([path,text])=>({path:r.id+'/'+path,text,size:new TextEncoder().encode(text).length,kind:path.endsWith('.md')?'markdown':'code',truncated:false,src:null}));}
function renderMetaInspector(){
 $('#meta-agent-context')?.remove();const session=metaSession(),r=metaAgent.resources.find(r=>r.uid===session?.metaResource);if(!r)return;
 $('#inspector-content').insertAdjacentHTML('afterbegin',`<div class="inspector-section" id="meta-agent-context"><div class="inspector-label">元Agent <span>${session.metaTest?'测试会话':'开发会话'}</span></div><div class="view-title">${escapeHTML(r.name)}</div><p class="view-subtitle">${metaKinds[r.kind]} · ${escapeHTML(session.metaTest?.version||r.version)} · ${session.metaTest?'固定快照 r'+session.metaTest.revision:'当前草稿 r'+r.revision}</p><div class="data-line"><span>开发助手</span><span>元Agent 开发助手</span></div><div class="data-line"><span>验证范围</span><span>本地结构检查</span></div><button class="small-link" data-nav="meta-agent">返回能力项目 ${icon('chevron-right')}</button></div>`);
}
function syncMetaFiles(profile){projectFilesCatalog[profile.project]={files:[{path:'README.md',kind:'markdown',text:'# 元Agent\n\n个人能力开发项目。源码草稿、测试快照与发布版本都保留在本次原型页面内。',size:150,truncated:false},...metaAgent.resources.filter(r=>r.projectId===profile.id).flatMap(r=>metaEntries(r,r.sources))]};}
function metaSnapshotEntries(){const session=metaSession();if(!session?.metaTest)return null;const resource=metaAgent.resources.find(r=>r.uid===session.metaResource);return [{path:'README.md',kind:'markdown',text:`# 测试快照 r${session.metaTest.revision}\n\n这是固定的源码快照，不随开发草稿修改。`,size:120,truncated:false},...metaEntries(resource,session.metaTest.sources)];}
function metaSessionChanges(){
 const session=metaSession(),r=metaAgent.resources.find(r=>r.uid===session?.metaResource);if(!r||session.metaTest)return [];
 if(r.changeProjection?.revision===r.revision)return r.changeProjection.files;
 const files=Object.entries(r.sources).filter(([path,text])=>text!==r.base[path]).map(([path,text])=>({name:path.split('/').at(-1),path:r.id+'/'+path,status:r.base[path]?'M':'A',origin:'turn',binary:false,added:text.split('\n').length,removed:(r.base[path]||'').split('\n').length,lines:[...(r.base[path]||'').split('\n').map((line,i)=>['del',String(i+1),line]),...text.split('\n').map((line,i)=>['add',String(i+1),line])]}));
 r.changeProjection={revision:r.revision,files};return files;
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button||button.disabled)return;
 if(button.dataset.metaSelect){activeProjectProfile().selected=button.dataset.metaSelect;renderMetaAgent();return;}
 if(button.dataset.metaView){metaAgent.tab=button.dataset.metaView;activeProjectProfile().metaTab=metaAgent.tab;renderMetaAgent();return;}
 if(button.dataset.metaFile){metaAgent.editor.path=button.dataset.metaFile;renderMetaEditorFile();return;}
 if(button.dataset.metaOpenTest){selectSession(button.dataset.metaOpenTest);return;}
 if(button.dataset.metaAcquire){const release=metaAgent.releases.find(r=>r.key===button.dataset.metaAcquire);navigate(release.kind==='package'?'packages':release.kind==='skill'?'skills':'extensions');const selector=release.kind==='package'?`[data-package-key="${release.catalogId}"]`:release.kind==='skill'?`[data-skill-id="${release.catalogId}"]`:`[data-extension-card="${release.catalogId}"]`;$(selector)?.scrollIntoView({block:'center'});return;}
 const action=button.dataset.metaAction;if(!action)return;const r=selectedMetaResource();
 if(action==='new-resource')newMetaResourceForm();
 if(action==='create-resource'){
  const name=$('#meta-resource-name').value.trim(),id=$('#meta-resource-id').value.trim(),kind=$('#meta-resource-kind').value;if(!name||!/^[a-z][a-z0-9.-]{2,79}$/.test(id)){toast('请填写名称和小写字母、数字、点或连字符组成的资源 ID');return;}if(metaAgent.resources.some(r=>r.id===id)){toast('资源 ID 已存在，请使用新的 ID');return;}
  const resource=createMetaResource(activeProjectProfile(),kind,name,id);activeProjectProfile().selected=resource.uid;metaAgent.tab='drafts';$('#modal').close();renderMetaAgent();
 }
 if(action==='develop'){if(state.running){toast('请先结束当前生成，再打开开发会话');return;}const session=metaDevSession(r);selectSession(session.id);$('#prompt').value=`请继续完善 ${r.name}，先阅读当前源码和测试场景。`;}
 if(action==='edit')metaOpenEditor();
 if(action==='save-source'){
  const resource=metaAgent.resources.find(r=>r.uid===metaAgent.editor.uid),drafts=metaAgent.editor.drafts;if(Object.values(drafts).some(text=>new TextEncoder().encode(text).length>200*1024)){toast('单文件不能超过 200 KiB');return;}
  if(JSON.stringify(resource.sources)!==JSON.stringify(drafts)){resource.sources=structuredClone(drafts);resource.revision++;resource.validation=null;if(resource.devSession)delete changeState.sessions[resource.devSession];try{const manifest=JSON.parse(drafts[resource.kind==='package'?'package.json':'extension.design.json']);if(/^\d+\.\d+\.\d+$/.test(manifest.version))resource.version=manifest.version;}catch{}syncMetaFiles(activeProjectProfile());}
  $('#modal').close();navigate('meta-agent');toast('已保存草稿，需要重新校验和测试');
 }
 if(action==='validate'){r.validation={revision:r.revision,...checkMetaSources(r)};renderMetaAgent();}
 if(action==='test')metaCreateTest(r);
 if(action==='publish')metaPublishForm(r);
 if(action==='confirm-publish')confirmMetaPublish();
 if(action==='iterate'){
  r.base=structuredClone(r.sources);r.version=r.version.replace(/\d+$/,value=>String(Number(value)+1));r.revision++;r.validation=null;if(r.devSession)delete changeState.sessions[r.devSession];
  const path=r.kind==='package'?'package.json':r.kind==='extension'?'extension.design.json':null;if(path){const manifest=JSON.parse(r.sources[path]);manifest.version=r.version;r.sources[path]=JSON.stringify(manifest,null,2);}syncMetaFiles(activeProjectProfile());renderMetaAgent();
 }
});
document.addEventListener('input',event=>{if(event.target.id==='meta-source-editor')metaAgent.editor.drafts[metaAgent.editor.path]=event.target.value;});
document.addEventListener('DOMContentLoaded',()=>{
 $('.primary-nav').insertAdjacentHTML('afterbegin',`<button id="meta-agent-nav" class="nav-item" data-nav="meta-agent" hidden>${icon('sparkles')}元Agent</button>`);
 $('#new-task').insertAdjacentHTML('beforebegin','<div id="added-projects" class="added-projects"></div>');renderProjectNavigation();
});
