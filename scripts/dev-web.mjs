#!/usr/bin/env node
import http from 'node:http';
import os from 'node:os';
import { gzipSync } from 'node:zlib';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const staticRoot = path.join(repoRoot, 'target', 'parity-web');
const port = Number(process.env.PORT || 4173);
const apiHost = process.env.RPI_API_HOST || '127.0.0.1';
const apiPort = Number(process.env.RPI_API_PORT || 3030);
const listenHost = process.env.RPI_UI_HOST || '0.0.0.0';
const allowedUiHosts = new Set([
  `localhost:${port}`, `127.0.0.1:${port}`, `[::1]:${port}`,
  ...Object.values(os.networkInterfaces()).flat().filter(address => address?.family === 'IPv4').map(address => `${address.address}:${port}`),
  ...(process.env.RPI_UI_ALLOWED_HOSTS || '').split(',').map(host => host.trim().toLowerCase()).filter(Boolean),
]);
const watchBuild = process.argv.includes('--watch-build');
const debugBuild = process.argv.includes('--debug-build') || process.env.RPI_DEBUG_BUILD === '1';
const reloadClients = new Set();
let lastBuildStamp = '';
let buildRunning = false;
let buildQueued = false;
let buildTimer;
const sourceExtensions = new Set(['.rs', '.css', '.toml', '.txt']);
const gzipCache = new Map();
const compressibleExtensions = new Set(['.wasm', '.js', '.css', '.html', '.json']);
const sourceTargets = [
  path.join(repoRoot, 'apps', 'work-ui'),
  path.join(repoRoot, 'crates', 'work-client'),
  path.join(repoRoot, 'crates', 'work-core'),
  path.join(repoRoot, 'crates', 'work-update'),
  path.join(repoRoot, 'Cargo.toml'),
  path.join(repoRoot, 'Cargo.lock'),
];

const mimeTypes = {
  '.css': 'text/css; charset=utf-8',
  '.d.ts': 'text/plain; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.wasm': 'application/wasm',
};

function log(message) {
  console.log(`[dev-web] ${message}`);
}

function broadcastReload() {
  for (const response of reloadClients) {
    response.write(`data: reload\n\n`);
  }
  log(`build changed; notified ${reloadClients.size} browser(s)`);
}

function currentBuildStamp() {
  try {
    const stat = fs.statSync(path.join(staticRoot, 'build.json'));
    return `${stat.mtimeMs}:${stat.size}`;
  } catch {
    return '';
  }
}

function watchBuildOutput() {
  const stamp = currentBuildStamp();
  if (lastBuildStamp && stamp && stamp !== lastBuildStamp) broadcastReload();
  lastBuildStamp = stamp;
}


function sourceStamp() {
  const entries = [];
  const visit = (target) => {
    let stat;
    try { stat = fs.statSync(target); } catch { return; }
    if (stat.isDirectory()) {
      for (const child of fs.readdirSync(target)) visit(path.join(target, child));
      return;
    }
    if (sourceExtensions.has(path.extname(target)) || path.basename(target) === 'Cargo.lock') {
      entries.push(`${target}:${stat.mtimeMs}:${stat.size}`);
    }
  };
  for (const target of sourceTargets) visit(target);
  return entries.sort().join('|');
}

let lastSourceStamp = sourceStamp();

function scheduleBuild() {
  clearTimeout(buildTimer);
  buildTimer = setTimeout(() => {
    const nextStamp = sourceStamp();
    if (nextStamp === lastSourceStamp) return;
    lastSourceStamp = nextStamp;
    startBuild();
  }, 500);
}

function proxyApi(req, response) {
  // Check the public request before translating its authority for the private
  // node. Rewriting arbitrary origins would remove the node's CSRF protection.
  const authority = (req.headers.host || '').toLowerCase();
  let origin;
  if (!allowedUiHosts.has(authority)) {
    response.writeHead(403, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ error: 'UI host is not allowed' }));
    return;
  }
  if (req.headers.origin) {
    try { origin = new URL(req.headers.origin); } catch { /* rejected below */ }
    if (!origin || !['http:', 'https:'].includes(origin.protocol) || origin.origin !== req.headers.origin || origin.host.toLowerCase() !== authority) {
      response.writeHead(403, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ error: 'UI origin must match this host' }));
      return;
    }
  }
  const headers = { ...req.headers, host: `${apiHost}:${apiPort}` };
  if (origin) headers.origin = `${origin.protocol}//${apiHost}:${apiPort}`;
  const proxy = http.request(
    {
      hostname: apiHost,
      port: apiPort,
      method: req.method,
      path: req.url,
      headers,
    },
    (upstream) => {
      response.writeHead(upstream.statusCode || 502, upstream.headers);
      upstream.pipe(response);
      response.on('close', () => upstream.destroy());
    },
  );
  proxy.on('error', (error) => {
    if (!response.headersSent) response.writeHead(502, { 'content-type': 'application/json; charset=utf-8' });
    response.end(JSON.stringify({ error: `local rpi-http unavailable: ${error.message}` }));
  });
  req.pipe(proxy);
  req.on('aborted', () => proxy.destroy());
  response.on('close', () => proxy.destroy());
}

function safeStaticPath(requestPath) {
  let decoded;
  try {
    decoded = decodeURIComponent(requestPath.split('?')[0]);
  } catch {
    return null;
  }
  // Keep the URL printed by build-ui-web.ps1 working while serving the same
  // bundle at the fixed-domain root used by the local reverse proxy.
  const bundlePrefix = '/target/parity-web';
  const normalized = decoded === bundlePrefix || decoded.startsWith(`${bundlePrefix}/`)
    ? decoded.slice(bundlePrefix.length) || '/'
    : decoded;
  const relative = normalized === '/' ? 'index.html' : normalized.replace(/^\/+/, '');
  const resolved = path.resolve(staticRoot, relative);
  return resolved === staticRoot || resolved.startsWith(`${staticRoot}${path.sep}`) ? resolved : null;
}

function serveStatic(req, response) {
  const filePath = safeStaticPath(req.url || '/');
  if (!filePath) {
    response.writeHead(400);
    response.end('bad path');
    return;
  }
  fs.stat(filePath, (statError, stat) => {
    if (statError || !stat.isFile()) {
      response.writeHead(404);
      response.end('not found');
      return;
    }
    fs.readFile(filePath, (readError, contents) => {
      if (readError) {
        response.writeHead(500);
        response.end('failed to read file');
        return;
      }
      let body = contents;
      let contentEncoding;
      const extension = path.extname(filePath);
      const type = mimeTypes[extension] || 'application/octet-stream';
      if (path.basename(filePath) === 'index.html') {
        const reloadScript = `<script>(()=>{const e=new EventSource('/__rpi_reload');e.onmessage=()=>location.reload();e.onerror=()=>setTimeout(()=>location.reload(),1500)})()</script>`;
        body = Buffer.from(contents.toString().replace('</body>', `${reloadScript}</body>`));
      }
      if (compressibleExtensions.has(extension) && /gzip/i.test(req.headers['accept-encoding'] || '')) {
        const plainBody = body;
        const cacheKey = `${filePath}:${stat.mtimeMs}:${plainBody.length}`;
        body = gzipCache.get(cacheKey);
        if (!body) {
          body = gzipSync(plainBody, { level: 9 });
          gzipCache.set(cacheKey, body);
        }
        contentEncoding = 'gzip';
      }
      const headers = {
        'cache-control': 'no-store',
        'content-type': type,
        'content-length': body.length,
      };
      if (contentEncoding) {
        headers['content-encoding'] = contentEncoding;
        headers.vary = 'Accept-Encoding';
      }
      response.writeHead(200, headers);
      if (req.method !== 'HEAD') response.end(body);
      else response.end();
    });
  });
}

function requestHandler(req, response) {
  const requestPath = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`).pathname;
  if (requestPath === '/__rpi_reload') {
    response.writeHead(200, {
      'cache-control': 'no-cache',
      connection: 'keep-alive',
      'content-type': 'text/event-stream; charset=utf-8',
      'access-control-allow-origin': '*',
    });
    response.write(': connected\n\n');
    reloadClients.add(response);
    req.on('close', () => reloadClients.delete(response));
    return;
  }
  if (requestPath === '/api' || requestPath.startsWith('/api/')) {
    proxyApi(req, response);
    return;
  }
  serveStatic(req, response);
}

function startBuild() {
  if (buildRunning) {
    buildQueued = true;
    return;
  }
  buildRunning = true;
  log('source change detected; rebuilding Web bundle');
  const child = spawn(
    'powershell.exe',
    ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(repoRoot, 'scripts', 'build-ui-web.ps1'), ...(debugBuild ? ['-DebugBuild'] : [])],
    { cwd: repoRoot, stdio: 'inherit', windowsHide: true },
  );
  child.on('exit', (code) => {
    buildRunning = false;
    if (code === 0) log('Web bundle rebuilt');
    else log(`Web build failed with exit code ${code ?? 'unknown'}; keeping previous bundle`);
    if (buildQueued) {
      buildQueued = false;
      startBuild();
    }
  });
}

function watchSources() {
  const watched = [
    path.join(repoRoot, 'apps', 'work-ui'),
    path.join(repoRoot, 'crates', 'work-client'),
    path.join(repoRoot, 'crates', 'work-core'),
    path.join(repoRoot, 'crates', 'work-update'),
    path.join(repoRoot, 'Cargo.toml'),
    path.join(repoRoot, 'Cargo.lock'),
  ];
  for (const target of watched) {
    try {
      fs.watch(target, { recursive: fs.statSync(target).isDirectory() }, scheduleBuild);
    } catch (error) {
      log(`cannot watch ${target}: ${error.message}`);
    }
  }
  log(`source watcher enabled (${debugBuild ? 'debug' : 'release'} build)`);
}

const server = http.createServer(requestHandler);
server.listen(port, listenHost, () => {
  log(`serving ${staticRoot}`);
  log(`local URL: http://127.0.0.1:${port}/`);
  log(`API proxy: /api/* -> http://${apiHost}:${apiPort}`);
  if (watchBuild) {
    watchSources();
    startBuild();
  }
});

setInterval(watchBuildOutput, 500);
setInterval(() => {
  for (const response of reloadClients) response.write(': keep-alive\n\n');
}, 15000);

function shutdown() {
  for (const response of reloadClients) response.end();
  server.close(() => process.exit(0));
}
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
