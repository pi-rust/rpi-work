// Real native host + real DLL; only the model response is a deterministic local fixture.
// Development adapter, NOT a production rpi-http extension-management endpoint.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { once, EventEmitter } from 'node:events';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.RPI_WORK_SMOKE_BIN || 'rpi';
const tool = 'work_demo_counter';
const packageName = 'rpi-work-extension-demo';
const verify = process.argv.includes('--verify');
await fs.mkdir(path.join(repo, 'target'), { recursive: true });
const root = await fs.mkdtemp(path.join(repo, 'target/extension-demo-'));
const project = path.join(root, 'project'), agent = path.join(root, 'agent');
await fs.mkdir(project); await fs.mkdir(agent);
const env = { ...process.env, RPI_CODING_AGENT_DIR: agent, RPI_OFFLINE: '1' };
for (const key of Object.keys(env)) if (/^(ANTHROPIC_|OPENAI_|RPI_(SESSION_ID|EXTENSIONS_DIR|PROVIDER|MODEL|BASE_URL|API_KEY)$)/.test(key)) delete env[key];
const token = crypto.randomBytes(32).toString('hex');
const state = { installed: false, loaded: false, busy: false, count: 0, completed: 0, steps: 4, sequence: 0, source: 'actual-rpi-tool-events', frames: [], checks: [], settlements: [], modelFixtureRequests: 0 };
const clients = new Set();
let rpc, base, input, closing = false, activeCall, startupInProgress = true, stopRequested = false;

class DemoStopped extends Error {}
function checkStop() { if (stopRequested) throw new DemoStopped('Stopped by user'); }
function finish() {
  stopRequested = true;
  if (startupInProgress || closing) return;
  cleanup().then(() => process.exit(0), error => { console.error(error); process.exit(1); });
}
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, finish);
if (!verify) {
  input = readline.createInterface({ input: process.stdin });
  input.on('line', line => { if (line.trim() === 'stop') { finish(); input.close(); } });
  input.on('close', finish);
}
async function deadline(promise, milliseconds, label) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(label)), milliseconds); })]); }
  finally { clearTimeout(timer); }
}

async function command(args, timeout = 180000) {
  const child = spawn(binary, args, { cwd: project, env, stdio: ['ignore', 'pipe', 'pipe'] });
  const chunks = [];
  child.stdout.on('data', chunk => chunks.push(chunk)); child.stderr.on('data', chunk => chunks.push(chunk));
  const timer = setTimeout(() => child.kill(), timeout);
  try {
    const [code] = await once(child, 'close');
    const output = Buffer.concat(chunks).toString('utf8');
    await fs.appendFile(path.join(root, 'commands.log'), `${JSON.stringify(args)}\n${output}\n`);
    assert.equal(code, 0, output);
    return output;
  } finally { clearTimeout(timer); }
}

class Rpc extends EventEmitter {
  constructor() {
    super(); this.nextId = 0; this.pending = new Map(); this.stderr = '';
    this.child = spawn(binary, ['--mode', 'rpc', '--project-only', '--approve', '--offline', '--no-session', '--no-skills', '--no-prompt-templates', '--no-context-files', '--no-builtin-tools', '--provider', 'work-demo-fixture', '--model', 'counter-model', '--api-key', 'demo-only', '--base-url', `${base}/v1`], { cwd: project, env, stdio: ['pipe', 'pipe', 'pipe'] });
    this.child.stderr.on('data', chunk => { this.stderr = (this.stderr + chunk).slice(-16000); });
    this.exited = new Promise(resolve => this.child.once('close', resolve));
    this.child.stdin.on('error', () => {}); // Individual writes reject their pending request.
    this.ready = new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`RPC ready timeout: ${this.stderr}`)), 20000);
      this.child.once('error', error => { clearTimeout(timer); reject(error); });
      this.child.once('close', () => { this.closed = true; clearTimeout(timer); reject(new Error(`RPC closed: ${this.stderr}`)); for (const pending of this.pending.values()) pending.reject(new Error('RPC closed')); this.pending.clear(); });
      readline.createInterface({ input: this.child.stdout }).on('line', line => {
        try {
          assert(line.length <= 1048576, 'oversized RPC frame');
          const frame = JSON.parse(line);
          if (frame.type === 'ready') { clearTimeout(timer); this.capabilities = frame; resolve(frame); }
          else if (frame.type === 'response') {
            const pending = this.pending.get(frame.id);
            if (pending) { this.pending.delete(frame.id); frame.success ? pending.resolve(frame.data) : pending.reject(new Error(JSON.stringify(frame.error))); }
          } else this.emit('frame', frame);
        } catch (error) { reject(error); }
      });
    });
  }
  async request(type, data = {}) {
    await this.ready;
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`RPC timeout: ${type}`)); }, 20000);
      this.pending.set(id, { resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); reject(error); } });
      this.child.stdin.write(`${JSON.stringify({ id, type, ...data })}\n`, error => { if (error) this.pending.get(id)?.reject(error); });
    });
  }
  async stop() {
    if (!this.closed) {
      try { await deadline(this.request('stop'), 1500, 'RPC stop timeout'); } catch { this.child.kill(); }
      const timer = setTimeout(() => this.child.kill(), 500);
      try { await deadline(this.exited, 3000, 'RPC process did not close after shutdown'); } finally { clearTimeout(timer); }
    }
    await fs.appendFile(path.join(root, 'host-stderr.log'), this.stderr);
  }
}

function manifest() {
  return { schemaVersion: 1, kind: 'rpi-work-extension-design', id: 'demo.counter-ui', name: 'Real DLL counter UI', version: '0.1.0', ui: { schemaVersion: 1, contributions: [{ id: 'counter', slot: 'session-inspector', title: 'Actual native tool progress', widgets: [
    { type: 'text', text: `Actual DLL count: ${state.count}` },
    { type: 'fields', fields: [{ label: 'Source', value: `rpi RPC / ${tool}` }, { label: 'Tool call', value: activeCall || 'Not started' }] },
    { type: 'progress', label: 'Completed ticks', value: state.completed, max: state.steps },
  ] }] } };
}
function snapshot() { return { ...state, frames: undefined, checks: undefined, manifest: manifest() }; }
function broadcast() { state.sequence++; const frame = snapshot(); for (const client of clients) client.write(`data: ${JSON.stringify(frame)}\n\n`); }
function observe(frame) {
  if (frame.type === 'tool_execution_start' && frame.toolName === tool) activeCall = frame.toolCallId;
  if (['tool_execution_update', 'tool_execution_end'].includes(frame.type) && frame.toolName === tool && frame.toolCallId === activeCall) {
    const details = (frame.partialResult || frame.result)?.details;
    if (details?.demo === 'rpi-work-counter-v1' && Number.isSafeInteger(details.count) && details.count >= state.count && Number.isInteger(details.completed) && details.completed >= 1 && details.completed <= 4 && details.steps === 4) {
      state.count = details.count; state.completed = details.completed;
      state.frames.push({ type: frame.type, toolCallId: activeCall, isError: frame.isError === true, ...details }); broadcast();
    }
  }
}
async function run() {
  if (state.busy) throw new Error('A demo run is already active');
  state.busy = true; state.completed = 0; activeCall = undefined; broadcast();
  let listener, timer;
  const settled = new Promise((resolve, reject) => {
    listener = frame => { if (frame.type === 'agent_settled') resolve(frame); };
    timer = setTimeout(() => reject(new Error('settlement timeout')), 15000);
    rpc.on('frame', listener);
  });
  try {
    const [settlement] = await Promise.all([settled, rpc.request('prompt', { message: 'Run the work_demo_counter tool once with exactly 4 steps.' })]);
    state.settlements.push(settlement);
    assert.equal(settlement.outcome, 'completed', `Agent did not complete: ${JSON.stringify(settlement)}`);
    assert(!settlement.error && !settlement.aborted, 'Agent settlement contains an error or abort');
    const end = state.frames.findLast(frame => frame.toolCallId === activeCall && frame.type === 'tool_execution_end');
    assert(end?.done === true && end.isError === false, 'actual tool has no successful final result');
    assert.equal(state.completed, 4, 'actual DLL did not complete 4 ticks');
  } catch (error) { state.failure = String(error); throw error; }
  finally { clearTimeout(timer); rpc?.off('frame', listener); state.busy = false; broadcast(); }
}

const server = http.createServer(async (request, response) => {
  try {
    const url = new URL(request.url, base || 'http://127.0.0.1');
    if (url.pathname === '/v1/chat/completions') {
      if (request.method !== 'POST') { response.writeHead(405).end(); return; }
      let text = ''; for await (const chunk of request) { text += chunk; assert(text.length < 1048576); }
      const body = JSON.parse(text); state.modelFixtureRequests++;
      const toolResult = body.messages.at(-1)?.role === 'tool';
      // Negative verification: a completed tool must not conceal a failed Agent run.
      if (toolResult && process.argv.includes('--fail-after-tool')) {
        response.writeHead(400, { 'Content-Type': 'application/json' }).end(JSON.stringify({ error: { message: 'Intentional final model response failure', type: 'invalid_request_error' } })); return;
      }
      if (!toolResult) assert(body.tools.some(item => item.function?.name === tool), 'fixture cannot request an unregistered tool');
      const delta = toolResult ? { content: 'The real native counter has completed.' } : { tool_calls: [{ index: 0, id: `demo-call-${state.modelFixtureRequests}`, type: 'function', function: { name: tool, arguments: '{"steps":4}' } }] };
      response.writeHead(200, { 'Content-Type': 'text/event-stream' });
      for (const choice of [{ delta, finish_reason: null }, { delta: {}, finish_reason: toolResult ? 'stop' : 'tool_calls' }]) response.write(`data: ${JSON.stringify({ id: 'local-fixture', object: 'chat.completion.chunk', created: 0, model: 'counter-model', choices: [{ index: 0, ...choice }] })}\n\n`);
      response.end('data: [DONE]\n\n'); return;
    }
    const origin = request.headers.origin;
    if (origin && !/^http:\/\/(127\.0\.0\.1|localhost):\d+$/.test(origin)) { response.writeHead(403).end(); return; }
    if (origin) response.setHeader('Access-Control-Allow-Origin', origin);
    response.setHeader('Vary', 'Origin'); response.setHeader('Cache-Control', 'no-store');
    if (request.method === 'OPTIONS') { response.writeHead(204, { 'Access-Control-Allow-Methods': 'GET, POST', 'Access-Control-Allow-Headers': 'Content-Type' }).end(); return; }
    if (url.searchParams.get('token') !== token) { response.writeHead(403).end(); return; }
    if (request.method === 'GET' && url.pathname === '/events') {
      response.writeHead(200, { 'Content-Type': 'text/event-stream' }); response.write(`data: ${JSON.stringify(snapshot())}\n\n`);
      clients.add(response); request.on('close', () => clients.delete(response)); return;
    }
    if (request.method === 'GET' && url.pathname === '/state') { response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(snapshot())); return; }
    if (request.method === 'POST' && url.pathname === '/run') {
      if (state.busy) { response.writeHead(409).end('Run already active'); return; }
      await run(); response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(snapshot())); return;
    }
    response.writeHead(404).end();
  } catch (error) { if (!response.headersSent) response.writeHead(500); response.end(String(error)); console.error(error); }
});

async function cleanup() {
  if (closing) return; closing = true;
  input?.close();
  try {
    if (rpc) { await rpc.stop(); rpc = undefined; state.loaded = false; }
    const registry = await fs.readFile(path.join(project, '.rpi/packages.json'), 'utf8').then(JSON.parse).catch(error => { if (error.code === 'ENOENT') return { packages: [] }; throw error; });
    state.installed = registry.packages.some(item => item.name === packageName);
    if (state.installed) {
      await command(['package', 'unlink', packageName]); state.installed = false;
      const probe = new Rpc(); rpc = probe; await probe.ready;
      const tools = await probe.request('get_available_tools');
      assert(!JSON.stringify(tools).includes(tool), 'unlinked DLL still loaded on fresh host');
      state.checks.push('unlink removes actual project package and fresh host no longer exposes tool');
      await probe.stop(); rpc = undefined;
    }
    state.cleanupVerified = true;
  } catch (error) {
    state.cleanupFailure = String(error); state.failure ||= String(error);
    if (rpc) { rpc.child.kill(); await deadline(rpc.exited, 3000, 'RPC cleanup timeout').catch(() => {}); rpc = undefined; }
    throw error;
  } finally {
    for (const client of clients) client.end();
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    await fs.mkdir(path.join(repo, 'artifacts'), { recursive: true });
    await fs.writeFile(path.join(repo, 'artifacts/extension-demo-validation.json'), JSON.stringify({ ...state, root, model: 'loopback deterministic fixture, no external model', productionLiveUiProtocol: false }, null, 2));
  }
}

try {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve)); base = `http://127.0.0.1:${server.address().port}`;
  await fs.writeFile(path.join(agent, 'models.json'), JSON.stringify({ providers: { 'work-demo-fixture': { baseUrl: `${base}/v1`, api: 'openai-completions', apiKey: 'demo-only', models: [{ id: 'counter-model', name: 'Local deterministic counter fixture', contextWindow: 32768, maxTokens: 1024 }] } } }));
  state.hostVersion = (await command(['--version'])).trim(); checkStop();
  await command(['package', 'link', packageName, '--path', path.join(repo, 'examples/extension-demo'), '--locked']); state.installed = true; checkStop();
  const registry = JSON.parse(await fs.readFile(path.join(project, '.rpi/packages.json'), 'utf8'));
  assert(registry.packages.some(item => item.name === packageName));
  const artifactDir = path.join(project, '.rpi/packages', packageName, 'extensions');
  const artifact = (await fs.readdir(artifactDir)).find(name => /\.(dll|so|dylib)$/.test(name)); assert(artifact);
  state.artifactSha256 = crypto.createHash('sha256').update(await fs.readFile(path.join(artifactDir, artifact))).digest('hex');
  state.checks.push('real rpi package link installed built cdylib with project registry and artifact hash');
  rpc = new Rpc(); const ready = await rpc.ready; checkStop(); assert.equal(ready.protocol, 'rpi'); rpc.on('frame', observe);
  const tools = await rpc.request('get_available_tools'); assert(JSON.stringify(tools).includes(tool), JSON.stringify(tools));
  const commands = await rpc.request('get_commands'); assert(JSON.stringify(commands).includes('work-demo-status'));
  const loaded = await rpc.request('execute_command', { command: 'work-demo-status' }); assert.equal(loaded.count, 0, JSON.stringify(loaded));
  state.loaded = true; state.checks.push('real host loaded DLL and executed registered extension command'); checkStop();
  if (verify) {
    await run(); checkStop(); assert.equal(state.count, 4); await run(); checkStop(); assert.equal(state.count, 8);
    assert(state.frames.filter(frame => frame.type === 'tool_execution_update').length >= 6);
    const status = await rpc.request('execute_command', { command: 'work-demo-status' }); assert.equal(status.count, 8);
    state.checks.push('actual tool lifecycle and partial RPC events completed twice; in-DLL command confirms count 8');
    await cleanup(); console.log('PASS', JSON.stringify(state.checks));
  } else {
    startupInProgress = false;
    console.log(`DEMO_READY ${JSON.stringify({ endpoint: `${base}/?token=${token}`, root, hostVersion: state.hostVersion })}`);
  }
} catch (error) {
  if (error instanceof DemoStopped) { state.stopped = true; console.log('STOPPED: cleaning isolated project'); }
  else { state.failure = String(error); console.error(error); process.exitCode = 1; }
  await cleanup();
}
