// Inspect actual compiled APK resources; resource paths may be obfuscated by AAPT.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const { chromium } = require('../support/browser.cjs');
const root = path.resolve(__dirname, '../..');
const apk = path.resolve(process.argv[2] || 'target/ci-dist/rpi-work-0.1.9-aarch64-linux-android-dev.apk');
const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
assert(sdk, 'Android SDK path required');
const aapt = path.join(sdk, 'build-tools/35.0.0', process.platform === 'win32' ? 'aapt.exe' : 'aapt');
const dump = (...args) => execFileSync(aapt, ['dump', ...args], { encoding:'utf8', maxBuffer:16*1024*1024 });
(async () => {
  const resources = dump('--values','resources',apk), manifest = dump('xmltree',apk,'AndroidManifest.xml'), badging = dump('badging',apk);
  assert(badging.includes("package: name='com.pirust.rpiwork'"));
  assert(badging.includes("sdkVersion:'24'"));
  assert(!badging.includes('application-debuggable'), 'Do not ship a debuggable package');
  const specs = new Map(), files = new Map();
  let config;
  const lines = resources.split(/\r?\n/);
  for (let i=0;i<lines.length;i++) {
    const spec = lines[i].match(/spec resource (0x[\da-f]+) [^:]+:([^:]+):/);
    if (spec) specs.set(spec[2],spec[1]);
    const configuration = lines[i].match(/^\s*config ([^:]+):/);
    if (configuration) config=configuration[1];
    const resource = lines[i].match(/^\s*resource (0x[\da-f]+) [^:]+:([^:]+):/);
    const file = lines[i+1]?.match(/\(string8\) "([^"]+)"/);
    if (resource && file) files.set(`${resource[2]}@${config}`,file[1]);
  }
  const iconId = specs.get('mipmap/ic_launcher'); assert(iconId);
  assert(new RegExp(`android:icon\\([^\\r\\n]*\\)=@${iconId}\\b`).test(manifest), 'Application must reference official launcher');
  assert(new RegExp(`android:roundIcon\\([^\\r\\n]*\\)=@${iconId}\\b`).test(manifest), 'Round launchers must use the same official mark');
  const adaptivePath=files.get('mipmap/ic_launcher@anydpi-v26'); assert(adaptivePath);
  const adaptive=dump('xmltree',apk,adaptivePath);
  assert(adaptive.includes('E: adaptive-icon'));
  const backgroundId=specs.get('color/rpi_launcher_background'),foregroundId=specs.get('drawable/rpi_launcher_foreground');
  assert(backgroundId && foregroundId, 'Canonical adaptive resources must exist');
  assert(new RegExp(`E: background[\\s\\S]*?android:drawable[^\\r\\n]*=@${backgroundId}\\b`).test(adaptive));
  assert(new RegExp(`E: foreground[\\s\\S]*?android:drawable[^\\r\\n]*=@${foregroundId}\\b`).test(adaptive));
  assert(new RegExp(`resource ${backgroundId} [^\\r\\n]+:color/rpi_launcher_background:[^\\r\\n]*d=0xfff4f1e9\\b`).test(resources));
  const comparisons=[];
  for (const density of ['mdpi','hdpi','xhdpi','xxhdpi','xxxhdpi']) {
    const compiled=files.get(`mipmap/ic_launcher@${density}`); assert(compiled,`${density}: legacy launcher is required`);
    comparisons.push({role:density,compiled,source:`mipmap-${density}/ic_launcher.png`});
  }
  const foreground=files.get('drawable/rpi_launcher_foreground@nodpi'); assert(foreground);
  comparisons.push({role:'adaptive-foreground',compiled:foreground,source:'drawable-nodpi/rpi_launcher_foreground.png'});
  const extracted=JSON.parse(execFileSync('python',['-c',
    'import sys,zipfile,json,base64,struct\nwith zipfile.ZipFile(sys.argv[1]) as z:\n names=json.loads(sys.argv[2]); libs=[n for n in z.namelist() if n.startswith("lib/") and n.endswith(".so")]; main=z.read("lib/arm64-v8a/libmain.so"); print(json.dumps({"frames":{n:base64.b64encode(z.read(n)).decode() for n in names},"abis":sorted(set(n.split("/")[1] for n in libs)),"elfMachine":struct.unpack_from("<H",main,18)[0]}))',
    apk,JSON.stringify(comparisons.map(c=>c.compiled))],{encoding:'utf8',maxBuffer:4*1024*1024}));
  assert.deepEqual(extracted.abis,['arm64-v8a']);assert.equal(extracted.elfMachine,183);
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const proofs=[];
  try {
    const page=await browser.newPage(); await page.route('http**://**',r=>r.abort());
    for (const item of comparisons) {
      const source=fs.readFileSync(path.join(root,'assets/branding/android',item.source)).toString('base64');
      const proof=await page.evaluate(async ({expected,actual})=>{
        async function read(data){const img=new Image();img.src=`data:image/png;base64,${data}`;await img.decode();const c=document.createElement('canvas');c.width=img.naturalWidth;c.height=img.naturalHeight;const ctx=c.getContext('2d');ctx.drawImage(img,0,0);return {width:c.width,height:c.height,pixels:ctx.getImageData(0,0,c.width,c.height).data};}
        const a=await read(expected),b=await read(actual);let maxDifference=0;
        if(a.width===b.width && a.height===b.height)for(let i=0;i<a.pixels.length;i++){if(i%4!==3 && a.pixels[i-i%4+3]===0 && b.pixels[i-i%4+3]===0)continue;maxDifference=Math.max(maxDifference,Math.abs(a.pixels[i]-b.pixels[i]));}
        return {expectedSize:[a.width,a.height],actualSize:[b.width,b.height],maxDifference};
      },{expected:source,actual:extracted.frames[item.compiled]});
      assert.deepEqual(proof.actualSize,proof.expectedSize,item.role);assert(proof.maxDifference<=2,`${item.role}: compiled brand differs from canonical SVG-derived resource`);
      proofs.push({...item,...proof});
    }
  } finally {await browser.close();}
  const sha256=createHash('sha256').update(fs.readFileSync(apk)).digest('hex');
  fs.writeFileSync(path.join(root,'artifacts/android-official-icon-apk.json'),JSON.stringify({passed:true,apk,sha256,abis:extracted.abis,elfMachine:extracted.elfMachine,iconId,adaptivePath,proofs,scope:'Actual APK manifest, compiled legacy/adaptive resources and ARM64 ELF; no physical-device installation or launcher screenshot acceptance'},null,2));
  console.log('PASS signed-package resource inspection: official legacy/adaptive/round launcher, six compiled pixel comparisons, ARM64 ELF, release package');
})().catch(error=>{console.error(error);process.exitCode=1;});
