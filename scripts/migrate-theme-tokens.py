"""One-time migration of prototype component CSS to semantic theme tokens."""
from pathlib import Path
import re, colorsys

files=['styles.css','runtime.css','conversation.css','capabilities.css','streaming.css','packages.css']
groups={
 'accent':['#b14b2a','#b97439','#8f4027'],
 'accent-bg':['#faeee6','#f1e7dd','#fbf1ec','#b14b2a08'],
 'selected-bg':['#e7e9e0'], 'selected-text':['#343c2f'],
 'warning':['#a77023','#856124','#706249','#8f8268','#9a7838','#b59655','#9e7c43','#a59473','#a17b32','#8a7b4b'],
 'warning-bg':['#f7f0df','#fdfaf2','#f4efdf','#dbc5a4'],
 'warning-line':['#e7d9bb','#c3ab91','#bf9e6d','#d0b28a','#d9bda8'],
 'danger':['#a35e50','#a16049','#ac6553','#ad6650','#a36a4d','#aa6956','#a96b4d','#9d6250'],
 'danger-bg':['#faf0eb'], 'danger-line':['#ead5ca','#e6cfc3','#e6cdbc','#e9d2c1'],
 'success':['#5f8b65','#6c8868','#457243','#3f755b','#829c6d','#8b9e78','#8a9d75','#82976a','#83956a','#879975','#8a9d77','#889c71','#93a77e','#7f936c'],
 'success-bg':['#edf5ec','#eaf2ec','#e7ece4'],
 'success-line':['#cbdcc8','#d6dfca','#acba9b','#acba98','#b8c5ad','#b9c5ae'],
 'info':['#50739a','#537da8','#6083a6','#5c7c9d','#7d84a9','#80708f'],
 'info-bg':['#edf2f7','#e6eefa','#e7eef6','#ece9f0'],
 'info-line':['#a6b9c7'],
 'terminal-bg':['#272d26'], 'terminal-text':['#ccd6c2'], 'terminal-line':['#3b4533','#3d4735'],
 'terminal-muted':['#9aab8d','#8b9a80'], 'terminal-command':['#e8eede'], 'terminal-success':['#a4c18f'],
 'toast-bg':['#30392d'], 'toast-text':['#f0f3eb'], 'toast-line':['#47543d'],
 'chrome-bg':['#e9eae3'], 'canvas':['#e5e6df'],
}
lookup={v:k for k,vs in groups.items() for v in vs}
def color_token(value,prop):
 value=value.lower()
 if value in lookup:return 'var(--'+lookup[value]+')'
 if len(value)==9:
  return 'var(--'+('focus-color' if 'outline' in prop or value.startswith('#b14b2a') else 'shadow-color' if 'shadow' in prop else 'backdrop')+')'
 if value in ['#fff','#ffffff']:return 'var(--card)'
 if 'border' in prop or prop=='scrollbar-color':return 'var(--line-strong)'
 if 'shadow' in prop:return 'var(--shadow-color)'
 h=value[1:];h=''.join(c*2 for c in h) if len(h)==3 else h
 rgb=[int(h[i:i+2],16)/255 for i in [0,2,4]];_,light,_=colorsys.rgb_to_hls(*rgb)
 if 'background' in prop:return 'var(--'+('code' if light>.8 else 'selected-bg')+')'
 return 'var(--'+('text' if light<.3 else 'muted')+')'
def declaration(match):
 prop,value=match.group(1),match.group(2)
 if prop.startswith('--'):return match.group(0)
 value=re.sub(r'#[\da-fA-F]{3,8}\b',lambda m:color_token(m[0],prop),value)
 if prop=='color':value=re.sub(r'\bwhite\b','var(--on-accent)',value)
 if prop in ['background','background-color']:value=re.sub(r'\bwhite\b','var(--thumb)',value)
 if prop=='border-radius':
  def radius(m):
   n=int(m[1]);name='xs' if n<=3 else 'sm' if n<=6 else 'md' if n<=9 else 'lg' if n<=13 else 'xl'
   return 'var(--radius-'+name+')'
  value=re.sub(r'(\d+)px',radius,value)
 if prop in ['padding','padding-top','padding-bottom','padding-left','padding-right','margin','margin-top','margin-bottom','margin-left','margin-right','gap','column-gap','row-gap']:
  def space(m):
   n=int(m[1]);token='var(--space-'+str(abs(n))+')'
   return ('calc('+token+' * -1)' if n<0 else token) if 0<abs(n)<=80 else m[0]
  value=re.sub(r'(?<![\d.])(-?\d+)px',space,value)
 value=value.replace('sans-serif','var(--sans)')
 return prop+':'+value
for filename in files:
 path=Path('prototype')/filename;s=path.read_text(encoding='utf-8')
 s=re.sub(r':root\{[^{}]*\}','',s)
 s=re.sub(r'[^{}]*\.theme-dark[^{}]*\{[^{}]*\}','',s)
 s=re.sub(r'([\w-]+):([^;{}]+)',declaration,s)
 path.write_text(s,encoding='utf-8')
