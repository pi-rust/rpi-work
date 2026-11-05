// Real rpi RPC hosts; deterministic loopback model, no external provider or user resources.
// Optional --native builds trusted repository demo sources offline in the isolated directory.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { EventEmitter, once } from 'node:events';
import fs from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.RPI_WORK_SMOKE_BIN || 'rpi';
await fs.mkdir(path.join(repo, 'target'), { recursive: true });
const root = await fs.mkdtemp(path.join(repo, 'target/resource-validation-'));
const project = path.join(root, 'project'), agent = path.join(root, 'agent');
const home = path.join(root, 'home'), sessions = path.join(root, 'sessions');
for (const dir of [project, agent, home, sessions]) await fs.mkdir(dir);
// Allowlist OS/process essentials, NOT ambient model/API-key/resource environment variables.
const env = {};
for (const [key, value] of Object.entries(process.env)) {
  if (/^(PATH|PATHEXT|SYSTEMROOT|WINDIR|COMSPEC|TEMP|TMP|SYSTEMDRIVE|NUMBER_OF_PROCESSORS|PROCESSOR_ARCHITECTURE)$/i.test(key)) env[key] = value;
}
Object.assign(env, { HOME: home, USERPROFILE: home, APPDATA: path.join(home, 'AppData/Roaming'), LOCALAPPDATA: path.join(home, 'AppData/Local'), XDG_CONFIG_HOME: path.join(home, '.config'), RPI_CODING_AGENT_DIR: agent, RPI_OFFLINE: '1', CARGO_NET_OFFLINE: 'true', CARGO_HOME: process.env.CARGO_HOME || path.join(os.homedir(), '.cargo'), RUSTUP_HOME: process.env.RUSTUP_HOME || path.join(os.homedir(), '.rustup'), CARGO_TARGET_DIR: path.join(root, 'build') });
const result = { root, binary, platform: `${process.platform}/${process.arch}`, model: 'loopback deterministic fixture', checks: [], observations: {}, requests: [], hosts: [], cleanup: false };
const hosts = new Set();
let base;
async function write(file, content) { await fs.mkdir(path.dirname(file), { recursive: true }); await fs.writeFile(file, content); }
async function deadline(promise, ms, label) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(label)), ms); })]); }
  finally { clearTimeout(timer); }
}
function check(name, condition) { assert(condition, name); result.checks.push(name); console.log('PASS', name); }
async function command(args) {
  const child = spawn(binary, args, { cwd: project, env, stdio: ['ignore', 'pipe', 'pipe'] });
  const chunks = [];
  child.stdout.on('data', chunk => chunks.push(chunk)); child.stderr.on('data', chunk => chunks.push(chunk));
  try {
    const [code] = await deadline(once(child, 'close'), 180000, 'command timeout');
    const output = Buffer.concat(chunks).toString('utf8');
    await fs.appendFile(path.join(root, 'commands.log'), `${JSON.stringify(args)}\n${output}\n`);
    assert.equal(code, 0, output); return output.trim();
  } finally { if (child.exitCode === null) child.kill(); }
}
class Rpc extends EventEmitter {
  constructor(label, args = []) {
    super(); this.id = 0; this.pending = new Map(); this.stderr = ''; this.frames = [];
    const argv = ['--mode', 'rpc', '--approve', '--offline', '--no-context-files', '--no-prompt-templates', '--session-dir', sessions, '--provider', 'resource-fixture', '--model', 'fixture-model', '--api-key', 'fixture-only', '--base-url', `${base}/v1`, ...args];
    this.child = spawn(binary, argv, { cwd: project, env, stdio: ['pipe', 'pipe', 'pipe'] });
    this.child.stdin.on('error', () => {});
    this.exited = new Promise(resolve => this.child.once('close', code => { this.closed = true; resolve(code); }));
    this.child.stderr.on('data', chunk => { this.stderr += chunk; });
    this.ready = new Promise((resolve, reject) => {
      this.child.once('error', reject);
      this.child.once('close', code => { reject(new Error(`RPC ${label} exited ${code}: ${this.stderr}`)); for (const pending of this.pending.values()) pending.reject(new Error('RPC closed')); this.pending.clear(); });
      readline.createInterface({ input: this.child.stdout }).on('line', line => {
        try {
          assert(line.length <= 1048576, 'oversized frame');
          const frame = JSON.parse(line); this.frames.push(frame);
          if (frame.type === 'ready') { this.capabilities = frame; resolve(frame); }
          else if (frame.type === 'response') { const pending = this.pending.get(frame.id); if (pending) { this.pending.delete(frame.id); pending.resolve(frame); } }
          else this.emit('frame', frame);
        } catch (error) { reject(error); this.parseError = String(error); }
      });
    });
    this.record = { label, args: argv, pid: this.child.pid }; result.hosts.push(this.record); hosts.add(this);
  }
  async response(type, data = {}) {
    await deadline(this.ready, 20000, `ready timeout: ${this.stderr}`);
    const id = ++this.id;
    try {
      return await deadline(new Promise((resolve, reject) => { this.pending.set(id, { resolve, reject }); this.child.stdin.write(`${JSON.stringify({ type, id, ...data })}\n`, error => { if (error) reject(error); }); }), 20000, `RPC timeout: ${type}`);
    } finally { this.pending.delete(id); }
  }
  async request(type, data) { const frame = await this.response(type, data); assert.equal(frame.success, true, JSON.stringify(frame)); return frame.data; }
  async prompt(message) {
    let listener;
    const settled = new Promise(resolve => { listener = frame => { if (frame.type === 'agent_settled') resolve(frame); }; this.on('frame', listener); });
    try {
      const [outcome] = await Promise.all([deadline(settled, 20000, 'settlement timeout'), this.request('prompt', { message })]);
      assert.equal(outcome.outcome, 'completed', JSON.stringify(outcome));
      assert(!outcome.error && !outcome.aborted, JSON.stringify(outcome));
      return outcome;
    } finally { this.off('frame', listener); }
  }
  async stop() {
    if (!this.closed) {
      try { await deadline(this.request('stop'), 1500, 'stop timeout'); } catch { this.child.kill(); }
      try { await deadline(this.exited, 3000, 'exit timeout'); }
      catch { this.child.kill(); await deadline(this.exited, 3000, 'kill timeout'); }
    }
    this.record.exitCode = await this.exited;
    this.record.parseError = this.parseError;
    await write(path.join(root, `${this.record.label}.json`), JSON.stringify(this.frames, null, 2));
    await write(path.join(root, `${this.record.label}.stderr.log`), this.stderr);
    hosts.delete(this);
  }
}
const server = http.createServer(async (request, response) => {
  try {
    assert.equal(request.method, 'POST'); assert.equal(request.url, '/v1/chat/completions');
    let text = ''; for await (const chunk of request) { text += chunk; assert(text.length < 1048576); }
    const body = JSON.parse(text); result.requests.push(body);
    const lastUser = body.messages.findLast(message => message.role === 'user');
    const toolResult = body.messages.at(-1)?.role === 'tool';
    const match = JSON.stringify(lastUser?.content).match(/EXECUTE_TOOL:([a-z_]+)/);
    if (match && !toolResult) assert(body.tools?.some(tool => tool.function.name === match[1]), 'requested tool unavailable');
    const call = match && !toolResult;
    const delta = call ? { tool_calls: [{ index: 0, id: `call-${result.requests.length}`, type: 'function', function: { name: match[1], arguments: '{"steps":2}' } }] } : { content: `FIXTURE_REPLY_${result.requests.length}` };
    response.writeHead(200, { 'Content-Type': 'text/event-stream' });
    for (const choice of [{ delta, finish_reason: null }, { delta: {}, finish_reason: call ? 'tool_calls' : 'stop' }]) response.write(`data: ${JSON.stringify({ id: 'fixture', object: 'chat.completion.chunk', created: 0, model: 'fixture-model', choices: [{ index: 0, ...choice }] })}\n\n`);
    response.end('data: [DONE]\n\n');
  } catch (error) { result.fixtureError = String(error); response.writeHead(500).end(String(error)); }
});
async function skill(dir, name, description) { await write(path.join(dir, name, 'SKILL.md'), `---\nname: ${name}\ndescription: ${description}\n---\n\nValidation-only instructions.\n`); }
function requestFor(marker) { const request = result.requests.findLast(body => JSON.stringify(body.messages.at(-1)?.content).includes(marker)); assert(request, `missing fixture request ${marker}`); return request; }
function system(body) { return JSON.stringify(body.messages.filter(message => message.role === 'system')); }
function tools(body) { return (body.tools || []).map(tool => tool.function.name).sort(); }
async function host(label, args) { const rpc = new Rpc(label, args); await deadline(rpc.ready, 20000, `ready timeout: ${label}`); return rpc; }

try {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve)); base = `http://127.0.0.1:${server.address().port}`;
  await write(path.join(agent, 'models.json'), JSON.stringify({ providers: { 'resource-fixture': { baseUrl: `${base}/v1`, api: 'openai-completions', apiKey: 'fixture-only', models: [{ id: 'fixture-model', contextWindow: 32768, maxTokens: 512 }] } } }));
  result.version = await command(['--version']);
  const skillA = path.join(root, 'skills-a'), skillB = path.join(root, 'skills-b');
  await skill(skillA, 'session-a', 'ONLY_SESSION_A'); await skill(skillB, 'session-b', 'ONLY_SESSION_B');
  await skill(path.join(project, '.rpi/skills'), 'collision', 'PROJECT_COLLISION_WINNER');
  await skill(path.join(agent, 'skills'), 'collision', 'GLOBAL_COLLISION_LOSER');
  await skill(path.join(agent, 'skills'), 'global-only', 'GLOBAL_ONLY_SKILL');
  const a = await host('a', ['--skill', skillA, '--system-prompt', 'PROMPT_SESSION_A']);
  const b = await host('b', ['--skill', skillB, '--system-prompt', 'PROMPT_SESSION_B']);
  result.observations.ready = a.capabilities;
  result.observations.initialTools = await a.request('get_available_tools');
  const bInitial = (await b.request('get_state')).activeTools;
  await a.request('set_active_tools', { tools: ['read'] }); await b.request('set_active_tools', { tools: ['read', 'bash'] });
  await Promise.all([a.prompt('ROUND_A_ONE'), b.prompt('ROUND_B_ONE')]);
  check('concurrent same-cwd hosts send independent active tool sets', JSON.stringify(tools(requestFor('ROUND_A_ONE'))) === '["read"]' && JSON.stringify(tools(requestFor('ROUND_B_ONE'))) === '["bash","read"]');
  check('same-cwd explicit Skills and system prompts stay independent at startup', system(requestFor('ROUND_A_ONE')).includes('ONLY_SESSION_A') && !system(requestFor('ROUND_A_ONE')).includes('ONLY_SESSION_B') && system(requestFor('ROUND_A_ONE')).includes('PROMPT_SESSION_A') && system(requestFor('ROUND_B_ONE')).includes('ONLY_SESSION_B') && !system(requestFor('ROUND_B_ONE')).includes('ONLY_SESSION_A') && system(requestFor('ROUND_B_ONE')).includes('PROMPT_SESSION_B'));
  check('project Skill wins same-name global collision; other global Skills load', system(requestFor('ROUND_A_ONE')).includes('PROJECT_COLLISION_WINNER') && !system(requestFor('ROUND_A_ONE')).includes('GLOBAL_COLLISION_LOSER') && system(requestFor('ROUND_A_ONE')).includes('GLOBAL_ONLY_SKILL'));
  await a.request('set_active_tools', { tools: ['read', 'bash'] }); await a.prompt('ROUND_A_TWO');
  check('idle tool change reaches next model request with previous history', JSON.stringify(tools(requestFor('ROUND_A_TWO'))) === '["bash","read"]' && JSON.stringify(requestFor('ROUND_A_TWO').messages).includes('ROUND_A_ONE') && JSON.stringify(requestFor('ROUND_A_TWO').messages).includes('FIXTURE_REPLY_'));
  check('changing A does not mutate live B', JSON.stringify((await b.request('get_state')).activeTools) === '["read","bash"]');
  await b.request('set_active_tools', { tools: ['bash'] }); await b.prompt('ROUND_B_NO_READ');
  check('disabling read hides Skill listing without proving Skills were unloaded', !system(requestFor('ROUND_B_NO_READ')).includes('<available_skills>'));
  await b.request('set_active_tools', { tools: ['read'] }); await b.prompt('ROUND_B_READ_RESTORED');
  check('reenabling read restores Skill listing without resource reload', system(requestFor('ROUND_B_READ_RESTORED')).includes('ONLY_SESSION_B'));
  await b.request('set_active_tools', { tools: [] });
  result.observations.emptyToolSelectionState = await b.request('get_available_tools');
  await b.prompt('ROUND_B_EMPTY_TOOLS');
  result.observations.emptyToolSelectionModelTools = tools(requestFor('ROUND_B_EMPTY_TOOLS'));
  check('empty activeTools means all registered tools at model boundary, not no tools', result.observations.emptyToolSelectionState.activeTools.length === 0 && result.observations.emptyToolSelectionModelTools.length > 0 && JSON.stringify(result.observations.emptyToolSelectionModelTools) === JSON.stringify([...result.observations.emptyToolSelectionState.tools].sort()));
  await skill(skillA, 'added-later', 'ADDED_AFTER_START');
  await a.prompt('ROUND_A_NEW_SKILL');
  result.observations.newSkillVisibleInExistingHost = system(requestFor('ROUND_A_NEW_SKILL')).includes('ADDED_AFTER_START');
  result.observations.reloadCommand = await a.response('execute_command', { command: '/reload' });
  result.observations.extensionCommands = await a.request('get_commands');
  check('new Skill is not auto-discovered by an existing RPC host', !result.observations.newSkillVisibleInExistingHost);
  check('stock RPC extension command dispatch does not provide TUI reload', !result.observations.reloadCommand.success && result.observations.reloadCommand.error.includes('unknown command'));
  const before = await a.request('get_state'); result.observations.beforeRestart = before;
  await a.stop();
  const resumed = await host('a-resumed', ['--session-id', before.sessionId, '--skill', skillB, '--system-prompt', 'PROMPT_AFTER_RESTART']);
  const restored = await resumed.request('get_messages'); result.observations.restoredMessages = restored;
  check('same session id restores previous transcript', JSON.stringify(restored).includes('ROUND_A_ONE') && JSON.stringify(restored).includes('ROUND_A_TWO') && JSON.stringify(restored).includes('FIXTURE_REPLY_'));
  result.observations.afterRestart = await resumed.request('get_state');
  result.observations.activeToolsPersisted = JSON.stringify([...before.activeTools].sort()) === JSON.stringify([...result.observations.afterRestart.activeTools].sort());
  result.observations.startupDefaultTools = bInitial;
  check('runtime tool selection is not automatically persisted across process restart', !result.observations.activeToolsPersisted && JSON.stringify(result.observations.afterRestart.activeTools) === JSON.stringify(bInitial));
  await resumed.request('set_active_tools', { tools: before.activeTools });
  await resumed.prompt('ROUND_A_RESUMED');
  check('caller can reapply saved tool selection after restart', JSON.stringify(tools(requestFor('ROUND_A_RESUMED'))) === '["bash","read"]');
  check('restart can change Skill/prompt configuration while model still receives old history', system(requestFor('ROUND_A_RESUMED')).includes('ONLY_SESSION_B') && !system(requestFor('ROUND_A_RESUMED')).includes('ONLY_SESSION_A') && system(requestFor('ROUND_A_RESUMED')).includes('PROMPT_AFTER_RESTART') && JSON.stringify(requestFor('ROUND_A_RESUMED').messages).includes('ROUND_A_ONE'));
  const fresh = await host('fresh-project-only', ['--project-only', '--skill', skillA]);
  await fresh.prompt('FRESH_RESOURCE_PROBE');
  check('fresh host discovers new Skill; project-only excludes global resources', system(requestFor('FRESH_RESOURCE_PROBE')).includes('ADDED_AFTER_START') && !system(requestFor('FRESH_RESOURCE_PROBE')).includes('GLOBAL_ONLY_SKILL'));
  await Promise.all([resumed.stop(), b.stop(), fresh.stop()]);

  if (process.argv.includes('--native')) {
    // Copy only trusted source + lockfile; never upgrade SDK dependencies in the checkout.
    for (const suffix of ['main', 'extra']) {
      const source = path.join(root, `native-${suffix}`);
      for (const file of ['Cargo.toml', 'Cargo.lock', 'src/lib.rs']) await write(path.join(source, file), await fs.readFile(path.join(repo, 'examples/extension-demo', file)));
      if (suffix === 'extra') {
        const file = path.join(source, 'src/lib.rs');
        await fs.writeFile(file, (await fs.readFile(file, 'utf8')).replaceAll('work_demo_counter', 'work_extra_counter').replaceAll('work-demo-status', 'work-extra-status'));
      }
      await command(['package', 'link', `validation-${suffix}`, '--path', source, '--locked', '--debug']);
    }
    // The extra native package is exposed as an independent extension, not a second package.
    const extraDir = path.join(project, '.rpi/packages/validation-extra/extensions');
    const filename = (await fs.readdir(extraDir)).find(name => /\.(dll|so|dylib)$/.test(name)); assert(filename);
    const extraFile = path.join(root, 'independent-tools', filename);
    await write(extraFile, await fs.readFile(path.join(extraDir, filename)));
    await command(['package', 'unlink', 'validation-extra']);
    await skill(path.join(project, '.rpi/packages/validation-main/skills'), 'main-package', 'MAIN_PACKAGE_SKILL');
    const nativeA = await host('native-a', ['--project-only', '--extension', extraFile]);
    const nativeB = await host('native-b', ['--project-only', '--no-builtin-tools']);
    result.observations.packageList = await command(['package', 'list']);
    result.observations.nativeTools = await nativeA.request('get_available_tools');
    check('one installed main Package composes with an explicit independent native extension', result.observations.packageList.includes('validation-main') && !result.observations.packageList.includes('validation-extra') && result.observations.nativeTools.tools.includes('work_demo_counter') && result.observations.nativeTools.tools.includes('work_extra_counter') && !(await nativeB.request('get_available_tools')).tools.includes('work_extra_counter'));
    for (const tool of ['work_demo_counter', 'work_extra_counter']) {
      await nativeA.request('set_active_tools', { tools: ['read', tool] }); await nativeA.prompt(`EXECUTE_TOOL:${tool}`);
    }
    check('both real native tools execute, not just register', (await nativeA.request('execute_command', { command: 'work-demo-status' })).count === 2 && (await nativeA.request('execute_command', { command: 'work-extra-status' })).count === 2);
    check('native Package Skill reaches model system prompt', system(result.requests.at(-1)).includes('MAIN_PACKAGE_SKILL'));
    check('native tool state is independent across same-project processes', (await nativeB.request('execute_command', { command: 'work-demo-status' })).count === 0);
    const nativeSessionId = (await nativeA.request('get_state')).sessionId;
    await nativeA.stop();
    const switched = await host('native-switched', ['--local-only', '--extension', extraFile, '--session-id', nativeSessionId]);
    const changedTools = await switched.request('get_available_tools');
    check('explicit-only extension set can change on restart with same native history', !changedTools.tools.includes('work_demo_counter') && changedTools.tools.includes('work_extra_counter') && JSON.stringify(await switched.request('get_messages')).includes('EXECUTE_TOOL:work_demo_counter'));
    await switched.prompt('NATIVE_AFTER_RESTART');
    check('changed native host continues old history in next model request', JSON.stringify(requestFor('NATIVE_AFTER_RESTART').messages).includes('EXECUTE_TOOL:work_demo_counter') && tools(requestFor('NATIVE_AFTER_RESTART')).includes('work_extra_counter') && !tools(requestFor('NATIVE_AFTER_RESTART')).includes('work_demo_counter'));
    result.observations.localOnlyRetainsProjectPackageSkills = system(requestFor('NATIVE_AFTER_RESTART')).includes('MAIN_PACKAGE_SKILL');
    check('local-only limits extensions, not project Package Skill discovery', result.observations.localOnlyRetainsProjectPackageSkills);
    check('concurrent other host retains its native Package and state', (await nativeB.request('get_available_tools')).tools.includes('work_demo_counter') && (await nativeB.request('execute_command', { command: 'work-demo-status' })).count === 0);
    await Promise.all([switched.stop(), nativeB.stop()]);
    await command(['package', 'unlink', 'validation-main']);
    const after = await host('after-unlink', ['--project-only', '--no-builtin-tools']);
    check('unlink removes Package tools on a fresh host', !(await after.request('get_available_tools')).tools.includes('work_demo_counter'));
    await after.stop();
  } else result.observations.native = 'NOT_RUN: pass --native for offline native demo builds';
  assert(!result.fixtureError, result.fixtureError);
} catch (error) { result.failure = String(error.stack || error); process.exitCode = 1; console.error(error); }
finally {
  const cleanup = await Promise.allSettled([...hosts].map(rpc => rpc.stop()));
  result.cleanup = cleanup.every(item => item.status === 'fulfilled') && hosts.size === 0;
  result.cleanupErrors = cleanup.filter(item => item.status === 'rejected').map(item => String(item.reason));
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
  await fs.mkdir(path.join(repo, 'artifacts'), { recursive: true });
  await write(path.join(root, 'result.json'), JSON.stringify(result, null, 2));
  await write(path.join(repo, 'artifacts/rpi-resource-validation.json'), JSON.stringify(result, null, 2));
  if (!result.cleanup) process.exitCode = 1;
  console.log(JSON.stringify({ version: result.version, checks: result.checks.length, cleanup: result.cleanup, evidence: path.join(root, 'result.json'), failure: result.failure }));
}
