'use strict';
// Verify every committed native raster against the canonical SVG, not only a hash.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { chromium } = require('../support/browser.cjs');
const root = path.resolve(__dirname, '../..');
const directory = path.join(root, 'assets/branding');
const read = name => fs.readFileSync(path.join(directory, name));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
(async () => {
  const metadata = JSON.parse(read('native-icons.json'));
  const svg = read('rpi.svg');
  const mark = read('rpi-gear.png');
  assert.equal(metadata.markSource, 'rpi-gear.png');
  assert.equal(metadata.markSourceSha256, sha256(mark));
  assert(svg.toString().includes(`href="data:image/png;base64,${mark.toString('base64')}"`),
    'packaging icons must embed the exact supplied logo used by the UI');
  assert.equal(metadata.sourceSha256, sha256(svg));
  for (const [name, expected] of Object.entries(metadata.hashes)) assert.equal(sha256(read(name)), expected, name);
  const frames = [{ name: 'PNG256', size: 256, bytes: read('rpi-256.png') }, { name: 'PNG512', size: 512, bytes: read('rpi-512.png') }];
  const ico = read('rpi.ico');
  assert.equal(ico.readUInt16LE(0), 0); assert.equal(ico.readUInt16LE(2), 1);
  assert.equal(ico.readUInt16LE(4), metadata.windowsSizes.length);
  metadata.windowsSizes.forEach((size, index) => {
    const entry = 6+16*index, width = ico[entry] || 256, height = ico[entry+1] || 256;
    assert.equal(width, size); assert.equal(height, size);
    assert.equal(ico.readUInt16LE(entry+6), 32);
    const offset = ico.readUInt32LE(entry+12), length = ico.readUInt32LE(entry+8);
    assert(offset >= 6+16*metadata.windowsSizes.length && offset+length <= ico.length);
    frames.push({ name: `ICO${size}`, size, bytes: ico.subarray(offset,offset+length) });
  });
  const icns = read('rpi.icns');
  assert.equal(icns.toString('ascii',0,4), 'icns'); assert.equal(icns.readUInt32BE(4), icns.length);
  const sizes = { ic07: 128, ic08: 256, ic09: 512, ic10: 1024 }, actualSizes = [];
  for (let offset=8;offset<icns.length;) {
    const type=icns.toString('ascii',offset,offset+4), length=icns.readUInt32BE(offset+4);
    assert(length>8 && offset+length<=icns.length && sizes[type]);
    actualSizes.push(sizes[type]); frames.push({ name:type,size:sizes[type],bytes:icns.subarray(offset+8,offset+length) });
    offset+=length;
  }
  assert.deepEqual(actualSizes,metadata.macosSizes);
  const android = JSON.parse(read('android-icons.json'));
  assert.equal(android.sourceSha256, sha256(svg));
  assert.equal(android.markSource, 'rpi-gear.png');
  assert.equal(android.markSourceSha256, sha256(mark));
  assert.deepEqual(android.adaptive, { layerPixels:432, markPixels:256, insetPixels:88 });
  for (const [name, expected] of Object.entries(android.hashes)) assert.equal(sha256(read(`android/${name}`)), expected, name);
  for (const [density,size] of [['mdpi',48],['hdpi',72],['xhdpi',96],['xxhdpi',144],['xxxhdpi',192]]) {
    frames.push({ name:`Android-${density}`,size,bytes:read(`android/mipmap-${density}/ic_launcher.png`) });
  }
  frames.push({ name:'Android-adaptive',size:432,markSize:256,inset:88,bytes:read('android/drawable-nodpi/rpi_launcher_foreground.png') });
  const adaptive = read('android/mipmap-anydpi-v26/ic_launcher.xml').toString();
  assert(adaptive.includes('@color/rpi_launcher_background') && adaptive.includes('@drawable/rpi_launcher_foreground'));
  assert(read('android/values/rpi_launcher.xml').toString().includes('#f4f1e9'));
  const browser = await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
  const results=[];
  try {
    for (const frame of frames) {
      // Isolate each size so Blink cannot reuse an embedded PNG raster from
      // a previously rendered SVG at another resolution.
      const page = await browser.newPage();
      await page.route('http**://**', route=>route.abort());
      const proof = await page.evaluate(async ({svg,png,size,markSize,inset})=>{
        async function pixels(uri, reference){const image=new Image();image.src=uri;await image.decode();const canvas=document.createElement('canvas');canvas.width=canvas.height=size;const ctx=canvas.getContext('2d');if(reference)ctx.drawImage(image,inset,inset,markSize,markSize);else ctx.drawImage(image,0,0,size,size);return {pixels:ctx.getImageData(0,0,size,size).data,width:image.naturalWidth,height:image.naturalHeight};}
        const sized=atob(svg).replace('<svg ',`<svg width="${markSize}" height="${markSize}" `);
        const expected=await pixels(`data:image/svg+xml;base64,${btoa(sized)}`, true), actual=await pixels(`data:image/png;base64,${png}`, false);
        let maxDifference=0;
        // PNG decode can round RGB in translucent edge pixels. Compare alpha
        // and premultiplied channels, which determine the displayed image.
        for(let i=0;i<actual.pixels.length;i+=4){
          const a=actual.pixels[i+3],b=expected.pixels[i+3];
          maxDifference=Math.max(maxDifference,Math.abs(a-b));
          for(let channel=0;channel<3;channel++){
            maxDifference=Math.max(maxDifference,Math.abs(
              Math.round(actual.pixels[i+channel]*a/255)-Math.round(expected.pixels[i+channel]*b/255)));
          }
        }
        return {width:actual.width,height:actual.height,maxDifference};
      },{svg:svg.toString('base64'),png:frame.bytes.toString('base64'),size:frame.size,markSize:frame.markSize??frame.size,inset:frame.inset??0});
      assert.equal(proof.width,frame.size);assert.equal(proof.height,frame.size);assert(proof.maxDifference<=2, `${frame.name}: SVG raster drift ${proof.maxDifference}`);
      results.push({name:frame.name,...proof});
      await page.close();
    }
  } finally {await browser.close();}
  fs.writeFileSync('artifacts/icon-alignment-native-assets.json',JSON.stringify({passed:true,sourceSha256:sha256(svg),frames:results.filter(r=>!r.name.startsWith('Android-')),scope:'Local SVG/raster/container equivalence; not macOS/Android installation acceptance'},null,2));
  fs.writeFileSync('artifacts/android-official-icon-assets.json',JSON.stringify({passed:true,sourceSha256:sha256(svg),frames:results.filter(r=>r.name.startsWith('Android-')),scope:'Committed legacy/adaptive pixel equivalence, not APK or device installation acceptance'},null,2));
  console.log(`PASS ${frames.length} desktop and Android icon frames match the exact Web SVG; hashes, safe-zone padding and dimensions verified`);
})().catch(error=>{console.error(error);process.exitCode=1;});
