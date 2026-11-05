// Rasterize the single Web brand SVG locally; never downloads images.
// Usage: node tools/dev/generate-brand-icons.cjs
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { chromium } = require('../../tests/support/browser.cjs');
const root = path.resolve(__dirname, '../..');
const folder = path.join(root, 'assets/branding');
(async () => {
  const source = fs.readFileSync(path.join(folder, 'rpi.svg'));
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH });
  const frames = new Map();
  let foreground;
  try {
    const page = await browser.newPage();
    await page.route('http**://**', route => route.abort());
    for (const size of [16, 24, 32, 48, 64, 72, 96, 128, 144, 192, 256, 512, 1024]) {
      const png = await page.evaluate(async ({ svg, size }) => {
        const image = new Image();
        image.src = `data:image/svg+xml;base64,${svg}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = size;
        canvas.getContext('2d').drawImage(image, 0, 0, size, size);
        return canvas.toDataURL('image/png').split(',')[1];
      }, { svg: source.toString('base64'), size });
      frames.set(size, Buffer.from(png, 'base64'));
    }
    // Android adaptive icons have a 108dp layer. Keep the entire official mark
    // in a centered 64dp box so launcher masks cannot crop the three brand bars.
    foreground = Buffer.from(await page.evaluate(async svg => {
      const image = new Image(); image.src = `data:image/svg+xml;base64,${svg}`;
      await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = 432;
      canvas.getContext('2d').drawImage(image, 88, 88, 256, 256);
      return canvas.toDataURL('image/png').split(',')[1];
    }, source.toString('base64')), 'base64');
  } finally { await browser.close(); }
  const androidFiles = new Map();
  for (const [density, size] of [['mdpi',48],['hdpi',72],['xhdpi',96],['xxhdpi',144],['xxxhdpi',192]]) {
    androidFiles.set(`mipmap-${density}/ic_launcher.png`, frames.get(size));
  }
  androidFiles.set('drawable-nodpi/rpi_launcher_foreground.png', foreground);
  androidFiles.set('values/rpi_launcher.xml', Buffer.from('<resources><color name="rpi_launcher_background">#f4f1e9</color></resources>\n'));
  androidFiles.set('mipmap-anydpi-v26/ic_launcher.xml', Buffer.from('<?xml version="1.0" encoding="utf-8"?>\n<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n    <background android:drawable="@color/rpi_launcher_background" />\n    <foreground android:drawable="@drawable/rpi_launcher_foreground" />\n</adaptive-icon>\n'));
  const androidHashes = {};
  for (const [file, bytes] of androidFiles) {
    const target = path.join(folder, 'android', file);
    fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, bytes);
    androidHashes[file] = createHash('sha256').update(bytes).digest('hex');
  }
  fs.writeFileSync(path.join(folder, 'android-icons.json'), JSON.stringify({
    source: 'rpi.svg', sourceSha256: createHash('sha256').update(source).digest('hex'),
    generator: 'tools/dev/generate-brand-icons.cjs', adaptive: { layerPixels: 432, markPixels: 256, insetPixels: 88 },
    hashes: androidHashes,
  }, null, 2) + '\n');
  const files = new Map([['rpi-256.png', frames.get(256)], ['rpi-512.png', frames.get(512)]]);
  const sizes = [16, 24, 32, 48, 64, 128, 256];
  const header = Buffer.alloc(6 + 16 * sizes.length);
  header.writeUInt16LE(1, 2); header.writeUInt16LE(sizes.length, 4);
  let offset = header.length;
  sizes.forEach((size, index) => {
    const pos = 6 + 16 * index, data = frames.get(size);
    header[pos] = header[pos + 1] = size === 256 ? 0 : size;
    header.writeUInt16LE(1, pos + 4); header.writeUInt16LE(32, pos + 6);
    header.writeUInt32LE(data.length, pos + 8); header.writeUInt32LE(offset, pos + 12);
    offset += data.length;
  });
  files.set('rpi.ico', Buffer.concat([header, ...sizes.map(size => frames.get(size))]));
  const chunks = [[128, 'ic07'], [256, 'ic08'], [512, 'ic09'], [1024, 'ic10']].map(([size, type]) => {
    const data = frames.get(size), header = Buffer.alloc(8);
    header.write(type); header.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([header, data]);
  });
  const icns = Buffer.alloc(8); icns.write('icns'); icns.writeUInt32BE(8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0), 4);
  files.set('rpi.icns', Buffer.concat([icns, ...chunks]));
  const hashes = {};
  for (const [file, bytes] of files) {
    fs.writeFileSync(path.join(folder, file), bytes);
    hashes[file] = createHash('sha256').update(bytes).digest('hex');
  }
  fs.writeFileSync(path.join(folder, 'native-icons.json'), JSON.stringify({
    source: 'rpi.svg', sourceSha256: createHash('sha256').update(source).digest('hex'),
    generator: 'tools/dev/generate-brand-icons.cjs', windowsSizes: sizes,
    macosSizes: [128, 256, 512, 1024], hashes,
  }, null, 2) + '\n');
  console.log('PASS generated native PNG / multi-resolution ICO / ICNS and Android legacy/adaptive icons from the canonical Web SVG; zero network requests');
})().catch(error => { console.error(error); process.exitCode = 1; });
