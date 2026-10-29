// Check repository boundaries and relocated references without invoking providers.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const files = directory => fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
  if (['node_modules', 'target', 'local'].includes(entry.name)) return [];
  const file = path.join(directory, entry.name);
  return entry.isDirectory() ? files(file) : [file];
});
let imports = 0, links = 0, includes = 0;
for (const file of [...files(path.join(root, 'tools')), ...files(path.join(root, 'tests'))]) {
  if (!/\.(?:cjs|mjs)$/.test(file)) continue;
  const text = fs.readFileSync(file, 'utf8');
  assert(!/(?:path\.resolve|require)\(['"]\.\.\/rpi-package\/packages\/rpi-web\/frontend/.test(text), `Neighboring browser dependency: ${file}`);
  for (const match of text.matchAll(/require\(['"](\.[^'"]+)['"]\)/g)) {
    assert(fs.existsSync(path.resolve(path.dirname(file), match[1])), `Missing local require in ${file}: ${match[1]}`);
    imports++;
  }
}
for (const file of files(path.join(root, 'apps/work-ui/src')).filter(file => file.endsWith('.rs'))) {
  const text = fs.readFileSync(file, 'utf8');
  for (const match of text.matchAll(/include_(?:str|bytes)!\("([^"]+)"\)/g)) {
    const target = path.resolve(path.dirname(file), match[1]);
    assert(fs.existsSync(target), `Missing embedded resource: ${target}`);
    assert(!target.startsWith(path.join(root, 'design') + path.sep), `Production depends on design: ${file}`);
    includes++;
  }
}
for (const file of [path.join(root, 'README.md'), ...files(path.join(root, 'docs')).filter(file => file.endsWith('.md'))]) {
  const text = fs.readFileSync(file, 'utf8');
  for (const match of text.matchAll(/\]\(([^)]+)\)/g)) {
    const value = match[1];
    if (value.includes('://') || /^[#<]|^codex:/.test(value)) continue;
    const target = value.split('#')[0];
    if (!target) continue;
    assert(fs.existsSync(path.resolve(path.dirname(file), target)), `Missing documentation link in ${file}: ${target}`);
    links++;
  }
}
const baseline = JSON.parse(fs.readFileSync(path.join(root, 'tests/fixtures/baselines/prototype-review-baseline.json'), 'utf8').replace(/^\uFEFF/, ''));
for (const [file, digest] of Object.entries(baseline.hashes)) {
  const actual = crypto.createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex');
  assert.equal(actual.toUpperCase(), digest.toUpperCase(), `Frozen design changed: ${file}`);
}
console.log(`PASS structure: ${imports} local JS imports, ${includes} embedded resources, ${links} documentation links, ${Object.keys(baseline.hashes).length} unchanged design files`);
