#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const outputRootArgumentIndex = process.argv.indexOf('--output-root');
const outputRootArgument = outputRootArgumentIndex >= 0 ? process.argv[outputRootArgumentIndex + 1] : '';
const outputRoot = path.resolve(repoRoot, outputRootArgument || process.env.RPI_UI_BUILD_OUTPUT_ROOT || 'target');
const outputDirectory = path.join(outputRoot, 'parity-web');
const serverDirectory = path.join(outputRoot, 'parity-web-server');
const debugBuild = process.argv.includes('--debug-build');
const profile = debugBuild ? 'debug' : 'release';

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: repoRoot, stdio: 'inherit' });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? resolve() : reject(new Error(`${command} exited with ${code ?? 'unknown'}`)));
  });
}

function runCapture(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: repoRoot, stdio: ['ignore', 'pipe', 'inherit'] });
    let output = '';
    child.stdout.on('data', chunk => { output += chunk; });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? resolve(output) : reject(new Error(`${command} exited with ${code ?? 'unknown'}`)));
  });
}

function collectSourceFiles() {
  const files = [path.join(repoRoot, 'Cargo.toml'), path.join(repoRoot, 'Cargo.lock')];
  const roots = ['apps', 'crates', 'assets'].map(name => path.join(repoRoot, name));
  const extensions = new Set(['.rs', '.css', '.toml', '.txt', '.md', '.js', '.svg', '.html', '.json']);
  const visit = target => {
    let stat;
    try { stat = fs.statSync(target); } catch { return; }
    if (stat.isDirectory()) {
      for (const child of fs.readdirSync(target)) visit(path.join(target, child));
    } else if (extensions.has(path.extname(target)) || path.basename(target) === 'Cargo.lock') {
      files.push(target);
    }
  };
  roots.forEach(visit);
  return [...new Set(files)].sort();
}

function sourceHashes() {
  const hashes = {};
  for (const file of collectSourceFiles()) {
    hashes[path.relative(repoRoot, file).split(path.sep).join('/')] = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex').toUpperCase();
  }
  return hashes;
}

function copyDirectoryContents(source, destination) {
  fs.mkdirSync(destination, { recursive: true });
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    fs.cpSync(path.join(source, entry.name), path.join(destination, entry.name), { recursive: true, force: true });
  }
}

async function main() {
  const before = sourceHashes();
  const bindgenVersion = /^wasm-bindgen\s+(.+)$/m.exec(await runCapture('wasm-bindgen', ['--version']))?.[1]?.trim();
  const lock = fs.readFileSync(path.join(repoRoot, 'Cargo.lock'), 'utf8');
  const versions = [...lock.matchAll(/^name = "wasm-bindgen"\nversion = "([^"]+)"/gm)].map(match => match[1]);
  if (new Set(versions).size !== 1 || bindgenVersion !== versions[0]) {
    throw new Error(`wasm-bindgen CLI must match Cargo.lock (${versions[0] || 'unknown'})`);
  }

  const cargoArgs = ['build', '--locked', '--target', 'wasm32-unknown-unknown', '-p', 'work-ui'];
  if (!debugBuild) cargoArgs.push('--release');
  await run('cargo', cargoArgs);
  const metadata = JSON.parse(await runCapture('cargo', ['metadata', '--format-version', '1', '--no-deps']));
  const wasmPath = path.join(metadata.target_directory, 'wasm32-unknown-unknown', profile, 'work-ui.wasm');
  fs.rmSync(outputDirectory, { recursive: true, force: true });
  fs.mkdirSync(outputDirectory, { recursive: true });
  await run('wasm-bindgen', [wasmPath, '--target', 'web', '--out-dir', outputDirectory, '--out-name', 'work-ui']);

  const after = sourceHashes();
  if (JSON.stringify(before) !== JSON.stringify(after)) throw new Error('Sources changed while building; rebuild after the current edit completes.');
  const wasm = fs.readFileSync(path.join(outputDirectory, 'work-ui_bg.wasm'));
  const wasmSha256 = crypto.createHash('sha256').update(wasm).digest('hex');
  const releaseId = `work-ui-${wasmSha256.slice(0, 16)}`;
  const releaseDirectory = path.join(outputDirectory, 'releases', releaseId);
  fs.rmSync(path.join(outputDirectory, 'releases'), { recursive: true, force: true });
  fs.mkdirSync(releaseDirectory, { recursive: true });
  for (const entry of fs.readdirSync(outputDirectory, { withFileTypes: true })) {
    if (['index.html', 'build.json', 'releases'].includes(entry.name)) continue;
    fs.cpSync(path.join(outputDirectory, entry.name), path.join(releaseDirectory, entry.name), { recursive: true, force: true });
  }
  fs.writeFileSync(path.join(outputDirectory, 'build.json'), JSON.stringify({
    profile,
    builtAt: new Date().toISOString(),
    sourceHashes: after,
    wasmSha256,
    assetPath: `releases/${releaseId}`,
  }, null, 2));
  fs.writeFileSync(path.join(outputDirectory, 'index.html'), `<!doctype html>\n<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"><title>RPI Work - Dioxus review</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from './releases/${releaseId}/work-ui.js'; await init();</script></body></html>\n`);

  const serverAssets = path.join(serverDirectory, 'assets');
  fs.rmSync(serverDirectory, { recursive: true, force: true });
  fs.mkdirSync(serverAssets, { recursive: true });
  for (const entry of fs.readdirSync(outputDirectory, { withFileTypes: true })) {
    if (['index.html', 'build.json'].includes(entry.name)) continue;
    fs.cpSync(path.join(outputDirectory, entry.name), path.join(serverAssets, entry.name), { recursive: true, force: true });
  }
  fs.copyFileSync(path.join(outputDirectory, 'build.json'), path.join(serverDirectory, 'build.json'));
  fs.writeFileSync(path.join(serverDirectory, 'index.html'), `<!doctype html>\n<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"><title>RPI Work</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from '/assets/releases/${releaseId}/work-ui.js'; await init();</script></body></html>\n`);
  console.log(`Review URL: http://127.0.0.1:14180/`);
}

main().catch(error => { console.error(error.stack || error); process.exitCode = 1; });
