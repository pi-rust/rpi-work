/* Workspace appearance only. Does not change providers, prompts or session configuration. */
const THEME_STORAGE='rpi-work-appearance-v1',THEME_LEGACY_STORAGE='rpi-work-prototype-theme';
const appearance={themes:[...THEME_PRESETS],saved:null,current:null,preview:null,previewThemes:null,committing:false,storageUnavailable:false};
function normalizeAppearance(value){return {theme:appearance.themes.some(t=>t.id===value?.theme)?value.theme:'paper',mode:['light','dark','system'].includes(value?.mode)?value.mode:'light',density:['standard','compact'].includes(value?.density)?value.density:'standard'};}
function readAppearance(){try{const saved=JSON.parse(localStorage.getItem(THEME_STORAGE)||'null');appearance.themes=[...THEME_PRESETS];for(const candidate of (Array.isArray(saved?.customThemes)?saved.customThemes.slice(0,12):[])){try{const t=validateAppearanceTheme({schemaVersion:1,kind:'rpi-work-theme',theme:candidate});if(!appearance.themes.some(p=>p.id===t.id))appearance.themes.push(t);}catch{}}return normalizeAppearance(saved||{mode:localStorage.getItem(THEME_LEGACY_STORAGE)||'light'});}catch{return normalizeAppearance({});}}
function resolvedAppearanceMode(config){return config.mode==='system'?(matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'):config.mode;}
function applyAppearance(config){
 config=normalizeAppearance(config);appearance.current=config;
 const theme=appearance.themes.find(t=>t.id===config.theme),mode=resolvedAppearanceMode(config),root=document.documentElement,tokens=buildThemeTokens(theme,mode,config.density);
 for(const [key,value] of Object.entries(tokens))root.style.setProperty('--'+key,value);
 root.dataset.theme=theme.id;root.dataset.colorMode=mode;root.dataset.appearance=config.mode;root.dataset.density=config.density;root.style.colorScheme=mode;
 document.body?.classList.toggle('theme-dark',mode==='dark');
 document.querySelector('meta[name="theme-color"]')?.setAttribute('content',theme.palettes[mode].bg);
 const label=document.querySelector('#theme-trigger-label');if(label)label.textContent=theme.name;
 return config;
}
function persistAppearance(){
 appearance.saved={...appearance.current};try{localStorage.setItem(THEME_STORAGE,JSON.stringify({...appearance.saved,customThemes:appearance.themes.filter(t=>!THEME_PRESETS.some(p=>p.id===t.id))}));localStorage.setItem(THEME_LEGACY_STORAGE,appearance.saved.mode);appearance.storageUnavailable=false;}catch{appearance.storageUnavailable=true;}
}
function applyTheme(mode){applyAppearance({...appearance.current,mode});}
function initThemeUI(){
 applyAppearance(appearance.current||appearance.saved);
 if(!document.querySelector('#theme-trigger'))document.querySelector('.top-actions').insertAdjacentHTML('afterbegin',`<button id="theme-trigger" class="theme-trigger" aria-label="切换界面主题" title="界面主题">${icon('sparkles')}<span id="theme-trigger-label">${escapeHTML(appearance.themes.find(t=>t.id===appearance.current.theme).name)}</span></button>`);
 if(!document.querySelector('#theme-import-input'))document.body.insertAdjacentHTML('beforeend','<input id="theme-import-input" type="file" accept=".json,application/json" hidden>');
}
function themePresetButton(theme){
 const mode=resolvedAppearanceMode(appearance.current),p=theme.palettes[mode];
 return `<button class="theme-preset ${appearance.current.theme===theme.id?'selected':''}" data-appearance="preset" data-id="${theme.id}" aria-pressed="${appearance.current.theme===theme.id}"><span class="theme-mini" style="--preview-bg:${p.bg};--preview-side:${p.side};--preview-card:${p.card};--preview-line:${p.line};--preview-accent:${p.accent};--preview-radius:${{soft:8,crisp:5,square:3}[theme.shape]}px"><i class="theme-mini-sidebar"><b></b><b></b><b></b></i><i class="theme-mini-content"><b></b><em></em><em></em><span></span></i></span><span class="theme-preset-label"><strong>${escapeHTML(theme.name)}</strong>${appearance.current.theme===theme.id?icon('check'):''}</span><small>${escapeHTML(theme.description)}</small></button>`;
}
function themeSettingsBody(){
 const theme=appearance.themes.find(t=>t.id===appearance.current.theme);
 return `<p class="theme-settings-intro">主题统一控制整个工作台。预览期间保留对话、草稿与会话资源配置。</p><div class="theme-preset-grid">${appearance.themes.map(themePresetButton).join('')}</div><div class="theme-setting-fields"><div class="field"><label for="theme-select">明暗模式</label><select id="theme-select"><option value="light" ${appearance.current.mode==='light'?'selected':''}>浅色</option><option value="dark" ${appearance.current.mode==='dark'?'selected':''}>深色</option><option value="system" ${appearance.current.mode==='system'?'selected':''}>跟随系统</option></select></div><div class="field"><label for="density-select">界面密度</label><select id="density-select"><option value="standard" ${appearance.current.density==='standard'?'selected':''}>标准 · 舒展阅读</option><option value="compact" ${appearance.current.density==='compact'?'selected':''}>紧凑 · 展示更多内容</option></select></div></div><div class="theme-contract-preview"><span><i style="background:var(--accent)"></i>品牌强调</span><span><i style="background:var(--success)"></i>成功 / 新增</span><span><i style="background:var(--danger)"></i>失败 / 删除</span><span><i style="background:var(--info)"></i>运行状态</span></div><div class="theme-token-summary"><span>字体 <b>${theme.font==='humanist'?'人文无衬线':'系统无衬线'}${theme.headingFont==='code'?' · 等宽标题':''}</b></span><span>轮廓 <b>${{soft:'柔和',crisp:'利落',square:'方正'}[theme.shape]}</b></span><span>范围 <b>整个工作台 · 本设备</b></span></div><div class="theme-portable"><button class="button" data-appearance="export">导出主题 JSON</button><button class="button" data-appearance="import">导入主题</button><span>预览中 · 完成后保存</span></div><p class="theme-error" id="theme-error" role="status"></p>`;
}
function settingsModal(){
 if(!appearance.preview){appearance.preview={...appearance.current};appearance.previewThemes=[...appearance.themes];}
 showModal('外观与主题',`<div id="theme-settings-body">${themeSettingsBody()}</div>`,`<button class="button" data-appearance="cancel">取消</button><button class="button primary" data-appearance="save">完成</button>`);
 document.querySelector('#modal').classList.add('theme-dialog');
}
function repaintThemeSettings(){
 const body=document.querySelector('#theme-settings-body');if(!body)return;
 const active=document.activeElement,id=active?.id,focusPreset=active?.dataset?.appearance==='preset'?active.dataset.id:null,scroll=document.querySelector('#modal').scrollTop;
 body.innerHTML=themeSettingsBody();document.querySelector('#modal').scrollTop=scroll;
 if(id)document.getElementById(id)?.focus();if(focusPreset)body.querySelector(`[data-id="${focusPreset}"]`)?.focus();
}
function closeThemePreview(save){
 if(save){persistAppearance();appearance.preview=null;appearance.previewThemes=null;appearance.committing=true;document.querySelector('#modal').close();appearance.committing=false;toast(appearance.storageUnavailable?'主题已应用，本浏览器未允许保存偏好':'已保存主题与明暗、密度偏好');}
 else{if(appearance.preview){appearance.themes=appearance.previewThemes||appearance.themes;applyAppearance(appearance.preview);}appearance.preview=null;appearance.previewThemes=null;document.querySelector('#modal').close();}
}
function exportAppearanceTheme(){
 const theme=appearance.themes.find(t=>t.id===appearance.current.theme),payload={schemaVersion:THEME_SCHEMA_VERSION,kind:'rpi-work-theme',theme,preferredMode:appearance.current.mode,density:appearance.current.density},url=URL.createObjectURL(new Blob([JSON.stringify(payload,null,2)+'\n'],{type:'application/json'})),a=document.createElement('a');
 a.href=url;a.download=`rpi-work-theme-${theme.id}.json`;document.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
function validateAppearanceTheme(data){
 const p=data?.theme,string=(s,max)=>typeof s==='string'&&s.trim().length>0&&s.length<=max;
 if(data?.schemaVersion!==1||data.kind!=='rpi-work-theme'||!p||!string(p.id,64)||!/^[a-z][a-z0-9-]*$/.test(p.id)||!string(p.name,40)||!string(p.description,160)||!Object.hasOwn(THEME_FONTS,p.font)||!Object.hasOwn(THEME_FONTS,p.headingFont)||!['soft','crisp','square'].includes(p.shape))throw Error('主题格式不正确，请使用版本 1 的主题 JSON。');
 const palettes={};for(const mode of ['light','dark']){const palette=p.palettes?.[mode];if(!palette||!Object.keys(THEME_PRESETS[0].palettes.light).every(key=>/^#[0-9a-fA-F]{6}$/.test(palette[key]||'')))throw Error('主题必须包含完整的浅色和深色颜色配置。');palettes[mode]=Object.fromEntries(Object.keys(THEME_PRESETS[0].palettes.light).map(key=>[key,palette[key]]));}
 for(const [mode,palette] of Object.entries(palettes)){
  if(['bg','side','panel','card','code'].some(bg=>['text','muted'].some(fg=>themeContrast(palette[fg],palette[bg])<4.5))||themeContrast(palette.accent,palette.card)<4.5||themeContrast(palette.accent,mode==='dark'?'#1c1c20':'#ffffff')<4.5)throw Error('主题文字对比度不足，请调整文字、强调色和背景色至至少 4.5:1。');
 }
 return {id:p.id,name:p.name,description:p.description,font:p.font,headingFont:p.headingFont,shape:p.shape,palettes};
}
async function importAppearanceTheme(file){
 if(!file)return;const preview=appearance.preview;
 try{if(file.size>64*1024)throw Error('主题 JSON 不能超过 64 KB。');const data=JSON.parse(await file.text()),theme=validateAppearanceTheme(data);
  if(!preview||appearance.preview!==preview||!document.querySelector('#theme-settings-body')||!document.querySelector('#modal').open)return;
  const existing=appearance.themes.find(t=>t.id===theme.id);if(existing&&JSON.stringify(existing)!==JSON.stringify(theme))throw Error('主题 ID 已存在且内容不同，请使用新的 ID。');
  if(!existing){if(appearance.themes.length>=THEME_PRESETS.length+12)throw Error('最多保存 12 个自定义主题。');appearance.themes.push(theme);}
  applyAppearance({theme:theme.id,mode:data.preferredMode||appearance.current.mode,density:data.density||appearance.current.density});repaintThemeSettings();
 }catch(e){const error=document.querySelector('#theme-error');if(error)error.textContent=e instanceof SyntaxError?'无法解析主题 JSON。':e.message;}
 finally{document.querySelector('#theme-import-input').value='';}
}
appearance.saved=readAppearance();applyAppearance(appearance.saved);
document.addEventListener('DOMContentLoaded',()=>{
 initThemeUI();
 document.querySelector('#modal').addEventListener('close',()=>{if(appearance.preview&&!appearance.committing){appearance.themes=appearance.previewThemes||appearance.themes;applyAppearance(appearance.preview);appearance.preview=null;appearance.previewThemes=null;}document.querySelector('#modal').classList.remove('theme-dialog');});
});
document.addEventListener('click',event=>{
 const b=event.target.closest('button');if(!b||b.disabled)return;if(b.id==='theme-trigger'){settingsModal();return;}
 if(b.dataset.appearance==='preset'){applyAppearance({...appearance.current,theme:b.dataset.id});repaintThemeSettings();}
 if(b.dataset.appearance==='save')closeThemePreview(true);
 if(b.dataset.appearance==='cancel')closeThemePreview(false);
 if(b.dataset.appearance==='export')exportAppearanceTheme();
 if(b.dataset.appearance==='import')document.querySelector('#theme-import-input').click();
});
document.addEventListener('change',event=>{
 if(event.target.id==='theme-select'){applyAppearance({...appearance.current,mode:event.target.value});repaintThemeSettings();}
 if(event.target.id==='density-select'){applyAppearance({...appearance.current,density:event.target.value});repaintThemeSettings();}
 if(event.target.id==='theme-import-input')importAppearanceTheme(event.target.files[0]);
});
matchMedia('(prefers-color-scheme: dark)').addEventListener('change',()=>{if(appearance.current.mode==='system'){applyAppearance(appearance.current);repaintThemeSettings();}});
window.addEventListener('storage',event=>{if(event.key===THEME_STORAGE&&!appearance.preview){appearance.saved=readAppearance();applyAppearance(appearance.saved);}});
