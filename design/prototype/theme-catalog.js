/* Theme contract: pure data + token resolver, shared by the browser and token generator. */
const THEME_SCHEMA_VERSION=1;
const THEME_FONTS={system:'"Segoe UI","Microsoft YaHei",sans-serif',humanist:'"Inter","Segoe UI","Microsoft YaHei",sans-serif',code:'"Cascadia Code","SFMono-Regular",Consolas,"Microsoft YaHei",monospace'};
const THEME_PRESETS=[
 {id:'paper',name:'纸与苔绿',description:'暖白底色、赤陶强调，柔和圆角。',font:'humanist',headingFont:'humanist',shape:'soft',palettes:{
  light:{bg:'#fcfcfa',side:'#f4f4f0',panel:'#f8f8f5',card:'#ffffff',text:'#282b27',muted:'#62685d',line:'#e1e4da',code:'#f1f2ee',accent:'#b14b2a',accentBg:'#faeee6',selected:'#e9ecdf'},
  dark:{bg:'#22261f',side:'#1d211b',panel:'#272c23',card:'#292f24',text:'#e1e5d9',muted:'#afb8a4',line:'#46503d',code:'#313a2a',accent:'#f2ab82',accentBg:'#413228',selected:'#35402d'}
 }},
 {id:'ocean',name:'海湾蓝',description:'冷白与海蓝，清晰边界和利落轮廓。',font:'system',headingFont:'system',shape:'crisp',palettes:{
  light:{bg:'#f7fafd',side:'#edf3f8',panel:'#eff5fa',card:'#ffffff',text:'#1d3246',muted:'#526a7b',line:'#d9e4ed',code:'#e8f0f6',accent:'#1267a1',accentBg:'#e4f1fb',selected:'#dcecf8'},
  dark:{bg:'#101f2d',side:'#0d1925',panel:'#142738',card:'#182e41',text:'#dfedf8',muted:'#a2bbcf',line:'#31516b',code:'#1e384c',accent:'#7fc7f5',accentBg:'#1e405a',selected:'#24445e'}
 }},
 {id:'graphite',name:'石墨紫',description:'中性灰紫、方正细节与等宽标题。',font:'system',headingFont:'code',shape:'square',palettes:{
  light:{bg:'#f9f9fc',side:'#f0eff5',panel:'#f3f2f8',card:'#ffffff',text:'#2f293d',muted:'#6d637c',line:'#e1dce9',code:'#eceaf3',accent:'#7550b3',accentBg:'#eee6fa',selected:'#e9e2f4'},
  dark:{bg:'#1e1c26',side:'#191720',panel:'#25222f',card:'#292634',text:'#ebe6f3',muted:'#bdb1ce',line:'#494057',code:'#342e43',accent:'#c8a6f5',accentBg:'#423252',selected:'#3d344c'}
 }},
 {id:'reddit',name:'Reddit 橙',description:'橙红强调、冷灰表面与炭黑夜间配色。',font:'system',headingFont:'system',shape:'soft',palettes:{
  light:{bg:'#ffffff',side:'#f3f5f6',panel:'#f8f9fa',card:'#ffffff',text:'#0f1a1c',muted:'#52646a',line:'#d6dde0',code:'#e9eef0',accent:'#c93600',accentBg:'#fff0e8',selected:'#e6ecef'},
  dark:{bg:'#0e1113',side:'#0b1416',panel:'#131b1f',card:'#181c1f',text:'#eef1f3',muted:'#adbac1',line:'#34444b',code:'#20272b',accent:'#ff6a33',accentBg:'#3b211a',selected:'#253035'}
 },brand:'#ff4500'}
];
function themeMix(a,b,weight){const values=[1,3,5].map(i=>Math.round(parseInt(a.slice(i,i+2),16)*(1-weight)+parseInt(b.slice(i,i+2),16)*weight).toString(16).padStart(2,'0'));return '#'+values.join('');}
function themeContrast(a,b){const luminance=hex=>{const values=hex.slice(1).match(/../g).map(v=>parseInt(v,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return values[0]*.2126+values[1]*.7152+values[2]*.0722;},x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05);}
function buildThemeTokens(preset,mode='light',density='standard'){
 const p=preset.palettes[mode],dark=mode==='dark',status=dark?{success:'#a1d8b6',successBg:'#203d31',successLine:'#467458',warning:'#f0d294',warningBg:'#3c3220',warningLine:'#79633b',danger:'#f1aaa2',dangerBg:'#462b2d',dangerLine:'#88504e',info:'#a6d0f5',infoBg:'#23394e',infoLine:'#4c6e90'}:{success:'#2d694d',successBg:'#eaf4ee',successLine:'#bfd8c7',warning:'#855b17',warningBg:'#fcf4df',warningLine:'#dfcb99',danger:'#a03c35',dangerBg:'#fff0ed',dangerLine:'#e9c5c1',info:'#285f90',infoBg:'#eaf2fa',infoLine:'#c4d8eb'};
 const colors={bg:p.bg,side:p.side,panel:p.panel,card:p.card,text:p.text,muted:p.muted,subtle:p.muted,line:p.line,code:p.code,accent:p.accent,'accent-bg':p.accentBg,'on-accent':dark?'#1c1c20':'#ffffff','line-strong':themeMix(p.line,p.muted,.3),'body-text':themeMix(p.text,p.muted,.15),'selected-bg':p.selected,'selected-text':p.text,canvas:p.side,'chrome-bg':themeMix(p.bg,p.side,.65),'focus-color':p.accent+'66','shadow-color':dark?'#00000055':'#202c2012',backdrop:dark?'#050b1477':'#17252f55',success:status.success,'success-bg':status.successBg,'success-line':status.successLine,warning:status.warning,'warning-bg':status.warningBg,'warning-line':status.warningLine,danger:status.danger,'danger-bg':status.dangerBg,'danger-line':status.dangerLine,info:status.info,'info-bg':status.infoBg,'info-line':status.infoLine,'diff-add':status.success,'diff-add-bg':status.successBg,'diff-del':status.danger,'diff-del-bg':status.dangerBg,'terminal-bg':dark?p.side:themeMix(p.text,'#000000',.15),'terminal-text':'#e1eaf2','terminal-line':'#526575','terminal-muted':'#b3c4d2','terminal-command':'#ffffff','terminal-success':'#a8dcba','toast-bg':dark?themeMix(p.card,p.accent,.18):p.text,'toast-text':dark?p.text:p.bg,'toast-line':dark?p.line:p.muted,thumb:dark?p.text:'#ffffff',green:status.success,'green-bg':status.successBg};
 const tokens={...colors,sans:THEME_FONTS[preset.font],mono:THEME_FONTS.code,'font-heading':THEME_FONTS[preset.headingFont],shadow:'0 16px 60px var(--shadow-color)','density-factor':density==='compact'?'.86':'1','font-message':density==='compact'?'12px':'13px','message-line-height':density==='compact'?'1.8':'1.95','control-height':density==='compact'?'34px':'39px','motion-fast':'160ms','motion-normal':'220ms'};
 Object.assign(tokens,{'accent-line':themeMix(p.line,p.accent,.45),'chart-system':p.accent,'chart-history':themeMix(p.bg,p.accent,.45),'chart-tools':status.warningLine,'chart-files':status.infoLine});
 Object.assign(tokens,{'qr-paper':'#ffffff','qr-ink':'#000000'});
 Object.assign(tokens,{'brand':preset.brand||p.accent,'on-brand':preset.brand?'#ffffff':colors['on-accent']});
 const radii={soft:[3,5,8,12,16],crisp:[2,3,5,8,10],square:[1,2,3,5,7]}[preset.shape];
 ['xs','sm','md','lg','xl'].forEach((key,i)=>tokens['radius-'+key]=radii[i]+'px');tokens['radius-pill']='999px';
 for(let n=1;n<=80;n++)tokens['space-'+n]=`calc(${n}px * var(--density-factor))`;
 return tokens;
}
