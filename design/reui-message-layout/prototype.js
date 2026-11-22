'use strict';
// Standalone review prototype: no NodeApi / provider requests.
const icons = {
  'chevron-down':'<path d="m6 9 6 6 6-6"/>', 'chevron-right':'<path d="m9 6 6 6-6 6"/>',
  plus:'<path d="M12 5v14M5 12h14"/>', x:'<path d="m6 6 12 12M18 6 6 18"/>',
  search:'<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4 4"/>',
  'panel-left':'<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>',
  'panel-right':'<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/>',
  folder:'<path d="M3 7V5h6l2 2h10v12H3Z"/>',
  file:'<path d="M14 3H5v18h14V8Z M14 3v5h5"/>',
  files:'<path d="M9 3h8l4 4v12H9Z M17 3v5h4 M5 7H3v14h12"/>',
  terminal:'<path d="m4 6 6 6-6 6M13 18h7"/>', check:'<path d="m5 12 4 4L19 6"/>',
  copy:'<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M15 8V4H4v11h4"/>',
  quote:'<path d="M4 6h6v7H7q0 4-3 5 M14 6h6v7h-3q0 4-3 5"/>',
  'arrow-up':'<path d="M12 19V5m-6 6 6-6 6 6"/>',
  'arrow-down':'<path d="M12 5v14m-6-6 6 6 6-6"/>',
  git:'<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="5" r="2"/><path d="M6 7v10M18 7q0 6-12 6"/>',
  activity:'<path d="M3 12h4l3-8 4 16 3-8h4"/>',
  moon:'<path d="M20.5 13A9 9 0 0 1 11 3.5 9 9 0 1 0 20.5 13Z"/>',
  sliders:'<path d="M4 7h6m4 0h6M4 17h10m4 0h2"/><circle cx="12" cy="7" r="2"/><circle cx="16" cy="17" r="2"/>',
  settings:'<path d="m9 3-1 3-3 1-2 4 2 3v4l4 3 3-1 3 1 4-3v-4l2-3-2-4-3-1-1-3Z"/><circle cx="12" cy="12" r="3"/>'
};
function renderIcons(root = document) {
  root.querySelectorAll('[data-icon]').forEach(node => {
    node.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${icons[node.dataset.icon] || icons.file}</svg>`;
  });
}
renderIcons();
const $ = selector => document.querySelector(selector);
// Stable scrollbars reserve a gutter on both sides of the transcript. Mirror
// that space in the pinned composer, including on non-overlay-scrollbar hosts.
const scrollArea = $('.transcript-scroll');
function syncScrollGutter() {
  const gutter = (scrollArea.offsetWidth - scrollArea.clientWidth) / 2;
  $('.conversation').style.setProperty('--scroll-gutter', gutter + 'px');
}
syncScrollGutter();
new ResizeObserver(syncScrollGutter).observe(scrollArea);
const defaults = { layout:'codex', divider:false, bleed:false, actions:true, activity:true, dark:false };
const storageKey = 'rpi-message-layout-review-v2';
let settings = {...defaults};
try {
  const saved = JSON.parse(localStorage.getItem(storageKey) || 'null');
  if (saved && typeof saved === 'object') {
    if (['codex','classic'].includes(saved.layout)) settings.layout = saved.layout;
    for (const key of Object.keys(defaults).filter(k => k !== 'layout')) {
      if (typeof saved[key] === 'boolean') settings[key] = saved[key];
    }
  }
} catch { /* Invalid or blocked storage must not break the preview. */ }
const activityExpansion = new WeakMap();
function applySettings() {
  // Closed <details> can hide content through the browser's internal content
  // visibility, even if its child has display:block. Actually open the group
  // in expanded mode; preserve its independent disclosure state on return.
  document.querySelectorAll('.activity-summary').forEach(group => {
    if (!settings.activity) {
      if (!activityExpansion.has(group)) activityExpansion.set(group, group.open);
      group.open = true;
    } else if (activityExpansion.has(group)) {
      group.open = activityExpansion.get(group);
      activityExpansion.delete(group);
    }
  });
  document.body.dataset.layout = settings.layout;
  document.body.dataset.theme = settings.dark ? 'dark' : 'light';
  for (const key of ['divider','bleed','actions','activity']) document.body.dataset[key] = String(settings[key]);
  document.querySelectorAll('[data-layout-pick]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.layoutPick === settings.layout)));
  document.querySelectorAll('[data-setting]').forEach(b => b.setAttribute('aria-checked', String(settings[b.dataset.setting])));
  try { localStorage.setItem(storageKey, JSON.stringify(settings)); } catch { /* In-memory fallback. */ }
}
applySettings();
let toastTimer;
function toast(message) {
  $('#toast').textContent = message;
  $('#toast').classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('#toast').classList.remove('visible'), 2200);
}
function setPanel(panel) {
  const visible = panel !== 'none';
  document.body.dataset.rail = panel;
  $('.right-rail').hidden = !visible;
  $('#run-content').hidden = panel !== 'run';
  $('#files-content').hidden = panel !== 'files';
  document.querySelectorAll('[data-panel]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.panel === panel)));
}
function showSettings() { $('.settings-dialog').showModal(); }
function closeDialog(selector) { $(selector).close(); }
async function copy(text) {
  try { await navigator.clipboard.writeText(text); toast('已复制'); }
  catch { toast('浏览器不允许读取剪贴板，请选中文字复制'); }
}
function copyableText(source) {
  if (!source) return '';
  const clone = source.cloneNode(true);
  clone.querySelectorAll('.line-no,.code-number,.message-actions,.code-caption,button.icon-button').forEach(node => node.remove());
  clone.querySelectorAll('br').forEach(node => node.replaceWith('\n'));
  clone.querySelectorAll('tr').forEach(row => row.replaceWith([...row.querySelectorAll('th,td')].map(cell=>cell.textContent).join('\t')+'\n'));
  clone.querySelectorAll('li').forEach(node => node.prepend('• '));
  clone.querySelectorAll('p,h1,h2,h3,li,blockquote,pre,.code-line').forEach(node => node.append('\n'));
  return clone.textContent.replace(/\n{3,}/g,'\n\n').trim();
}
function messageText(message) {
  return copyableText(message.querySelector('.user-bubble,.prose')) || message.dataset.copy || '';
}
function updateDraft() {
  const draft = $('#draft');
  $('.send-button').disabled = !draft.value.trim();
  draft.style.height = 'auto';
  draft.style.height = Math.min(160, Math.max(52, draft.scrollHeight)) + 'px';
  try { sessionStorage.setItem(storageKey + '-draft', draft.value); } catch { /* Optional persistence. */ }
}
try { $('#draft').value = sessionStorage.getItem(storageKey + '-draft') || ''; } catch { /* Empty draft. */ }
updateDraft();
function bottom() { $('.transcript-scroll').scrollTop = $('.transcript-scroll').scrollHeight; }
function renderSearch() {
  const query = $('#search').value.trim().toLowerCase();
  const root = $('#search-results');
  root.replaceChildren();
  const matches = [...document.querySelectorAll('.thread')].filter(b => b.querySelector('span').textContent.toLowerCase().includes(query));
  for (const item of matches) {
    const button = document.createElement('button');
    button.textContent = item.querySelector('span').textContent;
    button.addEventListener('click', () => { item.click(); closeDialog('.search-dialog'); });
    root.append(button);
  }
  if (!matches.length) { const p = document.createElement('p'); p.className = 'muted small'; p.textContent = '没有匹配的演示会话'; root.append(p); }
}
document.addEventListener('click', event => {
  const button = event.target.closest('button');
  if (!button) return;
  if (button.dataset.layoutPick) { settings.layout = button.dataset.layoutPick; applySettings(); return; }
  if (button.dataset.setting) { settings[button.dataset.setting] = !settings[button.dataset.setting]; applySettings(); return; }
  if (button.dataset.panel) { setPanel(document.body.dataset.rail === button.dataset.panel ? 'none' : button.dataset.panel); return; }
  if (button.dataset.openFile) { openPreviewFile(button.dataset.openFile); return; }
  if (button.classList.contains('thread')) { toast('演示会话：' + button.querySelector('span').textContent + '（本稿仅包含当前会话）'); return; }
  switch (button.dataset.action) {
    case 'settings': showSettings(); break;
    case 'close-settings': closeDialog('.settings-dialog'); break;
    case 'theme': settings.dark = !settings.dark; applySettings(); break;
    case 'reset-settings': settings = {...defaults}; applySettings(); break;
    case 'close-rail': setPanel('none'); break;
    case 'sidebar': document.body.classList.toggle('sidebar-hidden'); break;
    case 'bottom': bottom(); break;
    case 'copy': void copy(messageText(button.closest('.message'))); break;
    case 'copy-code': void copy(copyableText(button.closest('.tool-output,.code-block').querySelector('code'))); break;
    case 'quote': {
      const text = messageText(button.closest('.message'));
      $('#draft').value += ($('#draft').value ? '\n\n' : '') + text.split('\n').map(line=>'> '+line).join('\n') + '\n\n';
      updateDraft(); $('#draft').focus(); break;
    }
    case 'attach': $('#attachments').click(); break;
    case 'new': $('#draft').value = ''; updateDraft(); $('#draft').focus(); toast('演示稿：已清空输入框，不创建真实会话'); break;
    case 'search': renderSearch(); $('.search-dialog').showModal(); $('#search').focus(); break;
    case 'close-search': closeDialog('.search-dialog'); break;
  }
});
for (const dialog of document.querySelectorAll('dialog')) {
  dialog.addEventListener('click', event => {
    if (event.target !== dialog) return;
    const r = dialog.getBoundingClientRect();
    if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dialog.close();
  });
}
$('#draft').addEventListener('input', updateDraft);
$('#draft').addEventListener('keydown', event => {
  if (event.isComposing) return;
  if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); $('#composer').requestSubmit(); }
});
$('#composer').addEventListener('submit', event => {
  event.preventDefault();
  if (document.body.dataset.busy === 'true') return;
  const text = $('#draft').value.trim();
  if (!text) return;
  const turn = document.createElement('article');
  turn.className = 'turn'; turn.setAttribute('aria-label','演示新增轮次');
  // User text is never interpolated into markup.
  const user = document.createElement('div'); user.className = 'user-message message';
  const bubble = document.createElement('div'); bubble.className = 'user-bubble'; bubble.textContent = text; user.append(bubble);
  const answer = document.createElement('div'); answer.className = 'assistant-message';
  const p = document.createElement('p'); p.className = 'commentary'; p.textContent = '已在本地预览中添加这条消息。当前是 HTML 评审稿，未调用模型。'; answer.append(p);
  turn.append(user,answer); $('#messages').append(turn);
  $('#draft').value = ''; $('#attachments').value = ''; $('#attachment-list').replaceChildren(); $('#attachment-list').hidden = true; updateDraft(); bottom();
});
$('#attachments').addEventListener('change', () => {
  const list = $('#attachment-list'); list.replaceChildren(); list.hidden = !$('#attachments').files.length;
  for (const file of $('#attachments').files) {
    const label = document.createElement('span'); label.textContent = file.name + ' · 仅演示'; list.append(label);
  }
});
$('#model').addEventListener('change', () => { $('#runtime-model').textContent = $('#model').value; toast('演示模型已切换，不调用服务'); });
$('#thinking').addEventListener('change', () => { $('#runtime-thinking').textContent = $('#thinking').value; });
$('#search').addEventListener('input', renderSearch);
document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault();
    if (!$('.search-dialog').open && !$('.settings-dialog').open) { renderSearch(); $('.search-dialog').showModal(); $('#search').focus(); }
  }
});
// Expanded desktop starts at the beginning so both structure and first turn are reviewable.
