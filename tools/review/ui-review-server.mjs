// Local visual review only: expose reference, generated screenshots and compiled UI.
import http from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const port = Number(process.env.PORT || 4182);
const mounts = [['/prototype/', 'design/prototype'], ['/design/prototype/', 'design/prototype'], ['/assets/', 'assets'], ['/artifacts/', 'artifacts'], ['/app/', 'target/parity-web'], ...['all', 'meta', 'resources', 'trace', 'project', 'extras', 'conversation', 'settings', 'queue', 'scroll'].map(feature => [`/modules/${feature}/`, `target/parity-modules-${feature}-web`])];
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml', '.wasm': 'application/wasm' };
http.createServer(async (request, response) => {
  try {
    if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405); response.end(); return; }
    const url = new URL(request.url, 'http://localhost');
    const mount = mounts.find(([prefix]) => url.pathname.startsWith(prefix));
    if (!mount) { response.writeHead(404); response.end('Review content not found'); return; }
    const [prefix, directory] = mount;
    const base = path.resolve(root, directory);
    const file = path.resolve(base, decodeURIComponent(url.pathname.slice(prefix.length) || 'index.html'));
    if (!file.startsWith(base + path.sep)) { response.writeHead(403); response.end(); return; }
    const bytes = await fs.readFile(file);
    response.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
    response.end(request.method === 'HEAD' ? undefined : bytes);
  } catch { response.writeHead(404); response.end('Review content not found'); }
}).listen(port, '127.0.0.1', () => console.log(`Visual review: http://127.0.0.1:${port}/prototype/`));
