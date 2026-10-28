// Build a review gallery from the current, measured screenshots. No fixture data is published.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..');
const read = file => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8').replace(/^\uFEFF/, ''));
const audit = read('artifacts/ui-pages-review-all.json');
const geometryRows = read('artifacts/ui-responsive-geometry-review.json');
for (const row of Object.values(geometryRows)) assert.deepEqual(row.actual, row.reference);
const geometry = { matched: Object.keys(geometryRows).length, regions: Object.keys(geometryRows).length };
const build = read('target/parity-web/build.json');
for (const [file, digest] of Object.entries(build.sourceHashes)) {
  const actual = crypto.createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex');
  if (actual.toUpperCase() !== digest.toUpperCase()) throw Error(`Rebuild stale source: ${file}`);
}
if (audit.build.wasmSha256 !== build.wasmSha256) throw Error('Screenshot audit is stale');
const failed = audit.results.filter(row => row.error || row.actual?.layout?.viewportOverflow || row.actual?.layout?.overflow?.length);
if (failed.length) throw Error(`Fix visual audit failures: ${JSON.stringify(failed)}`);
const pairs = audit.results.filter(row => !row.notApplicable);
for (const row of pairs) for (const side of ['actual', 'reference']) {
  if (!fs.existsSync(path.join(root, `artifacts/page-review-${row.name}-${row.width}-${side}.png`))) throw Error('Missing screenshot');
}
const data = JSON.stringify(audit.results).replace(/</g, '\\u003c');
const html = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>RPI Work · 样式还原对照</title>
<style>[hidden]{display:none!important}*{box-sizing:border-box}body{margin:0;background:#f3f5f6;color:#17232b;font:14px/1.6 system-ui,sans-serif}header{padding:24px;max-width:1600px;margin:auto}h1{font-size:24px;margin:0 0 8px}p{margin:6px 0;color:#536773}a{color:#bd3700}nav{position:sticky;top:0;z-index:1;display:flex;flex-wrap:wrap;align-items:center;gap:12px;padding:12px 24px;background:white;border-block:1px solid #d1d9df}select,button{font:inherit;padding:7px 12px;background:white;border:1px solid #bbc8cf;border-radius:6px}label{display:flex;align-items:center;gap:8px}main{padding:24px;max-width:3000px;margin:auto}.pair{display:grid;grid-template-columns:1fr 1fr;gap:20px}.panel{min-width:0;background:white;border:1px solid #d1d9df;border-radius:8px;overflow:hidden}.panel h2{font-size:13px;margin:0;padding:10px 14px;border-bottom:1px solid #d1d9df}.panel img{display:block;width:100%;height:auto}#notice{padding:14px;background:#fff1e9;border-radius:8px;margin-bottom:16px}#overlay{position:relative;max-width:1440px;margin:auto}#overlay img{display:block;width:100%;height:auto}#overlay .layer{position:absolute;inset:0;opacity:.5}small{color:#536773}@media(max-width:700px){.pair{grid-template-columns:1fr}main,header{padding:16px}nav{padding:10px 16px}}</style>
<header><h1>RPI Work · 样式还原对照</h1><p>${pairs.length} 组实现 / 原型截图，覆盖 1440、1280、768、390、320 像素；${audit.results.length-pairs.length} 项按已确认的设备流程标记为不适用。</p><p>主界面 ${geometry.matched}/${geometry.regions} 项布局与字号检查一致；可见卡片、导航、弹窗和输入栏未检测到横向溢出。页面内容、真实节点能力及已确认的新交互与早期原型有差异，本报告不代表整张截图逐像素相同。</p><small>构建 ${build.builtAt} · 截图 ${audit.capturedAt}</small><p><a href="/app/">打开当前界面</a> · <a href="/prototype/">打开原型</a> · <a href="ui-pages-review-all.json">页面测量</a> · <a href="ui-responsive-geometry-review.json">布局测量</a></p></header>
<nav><label>页面 <select id="page"></select></label><label>宽度 <select id="width"><option>1440</option><option>1280</option><option>768</option><option>390</option><option>320</option></select></label><label><input id="mode" type="checkbox">叠加对照</label><label id="mixLabel" hidden>实现透明度 <input id="mix" type="range" min="0" max="100" value="50"></label></nav>
<main><div id="notice" hidden></div><div class="pair" id="pair"><section class="panel"><h2>当前 Dioxus 实现</h2><img id="actual" alt="当前实现"></section><section class="panel"><h2>已确认 HTML 原型</h2><img id="reference" alt="原型"></section></div><div id="overlay" hidden><img id="base" alt="原型"><img id="layer" class="layer" alt="叠加的实现"></div></main>
<script>const rows=${data};const $=id=>document.getElementById(id);for(const name of [...new Set(rows.map(r=>r.name))]){const o=document.createElement('option');o.value=name;o.textContent=name;$('page').appendChild(o)}function render(){const row=rows.find(r=>r.name===$('page').value&&r.width===+$('width').value);const na=!row||row.notApplicable;$('notice').hidden=!na;$('notice').textContent=na?'不适用：当前设备不提供此入口，电脑展示地址二维码，手机扫码或手动输入地址。':'';$('pair').hidden=na||$('mode').checked;$('overlay').hidden=na||!$('mode').checked;$('mixLabel').hidden=!$('mode').checked;if(na)return;for(const [id,side]of [['actual','actual'],['reference','reference'],['base','reference'],['layer','actual']])$(id).src='page-review-'+row.name+'-'+row.width+'-'+side+'.png';$('layer').style.opacity=$('mix').value/100}for(const id of ['page','width','mode','mix'])$(id).addEventListener('input',render);render();</script></html>`;
fs.writeFileSync(path.join(root, 'artifacts/style-restoration-gallery.html'), html);
console.log(`Gallery: ${pairs.length} pairs, ${geometry.matched}/${geometry.regions} layout checks`);
