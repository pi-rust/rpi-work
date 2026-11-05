// Opt-in live ACP smoke. Uses existing agent auth, never reads or prints credentials.
// node tools/review/smoke-acp-agents.mjs --adapters target/acp-tools [--prompt]
// --prompt makes at most one short, no-tools model request per backend (may incur cost).
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';
const args = process.argv.slice(2), prompt = args.includes('--prompt');
const adaptersIndex = args.indexOf('--adapters');
const adapters = path.resolve(adaptersIndex >= 0 ? args[adaptersIndex + 1] : 'target/acp-tools');
const only = args.indexOf('--backend');
const backends = only >= 0 ? [args[only + 1]] : ['claude', 'codex', 'opencode'];
const reports = [];
const root = fs.mkdtempSync(path.resolve('target/acp-live-'));
const summarizeError = value => JSON.stringify(value).replace(/(?:sk-|Bearer\s+)[\w-]+/gi, '[REDACTED]').slice(0, 1500);
for (const backend of backends) {
    const cwd = path.join(root, backend); fs.mkdirSync(cwd);
    const prefix = `RPI_WORK_${backend.toUpperCase()}_ACP`;
    const override = process.env[`${prefix}_BIN`];
    const executable = override || (backend === 'opencode' ? 'opencode' : process.execPath);
    const commandArgs = process.env[`${prefix}_ARGS`] ? JSON.parse(process.env[`${prefix}_ARGS`]) :
        backend === 'opencode' ? ['acp'] : [path.join(adapters, 'node_modules', '@agentclientprotocol', backend === 'claude' ? 'claude-agent-acp' : 'codex-acp', 'dist/index.js')];
    const report = { backend, initialize: false, session: false, promptRequested: prompt, promptSucceeded: false, cancelledPermissions: 0 };
    let child, lines, sessionId, lastId = 0, text = '', stderrBytes = 0;
    const pending = new Map();
    const send = value => child.stdin.write(JSON.stringify(value) + '\n');
    const request = (method, params, milliseconds = 30000) => new Promise((resolve, reject) => {
        const id = ++lastId;
        const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, milliseconds);
        pending.set(id, { resolve, reject, timer });
        send({ jsonrpc: '2.0', id, method, params });
    });
    try {
        child = spawn(executable, commandArgs, { cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, shell: false });
        child.stderr.on('data', chunk => { stderrBytes += chunk.length; });
        child.on('error', error => { for (const task of pending.values()) { clearTimeout(task.timer); task.reject(error); } pending.clear(); });
        child.on('exit', code => { for (const task of pending.values()) { clearTimeout(task.timer); task.reject(new Error(`ACP process exited: ${code}`)); } pending.clear(); });
        lines = readline.createInterface({ input: child.stdout });
        lines.on('line', line => {
            let value; try { value = JSON.parse(line); } catch { return; }
            if (value.method) {
                if (value.method === 'session/update' && value.params?.sessionId === sessionId && value.params.update?.sessionUpdate === 'agent_message_chunk') text += value.params.update.content?.text || '';
                if (value.id !== undefined) {
                    if (value.method === 'session/request_permission') {
                        report.cancelledPermissions++;
                        send({ jsonrpc: '2.0', id: value.id, result: { outcome: { outcome: 'cancelled' } } });
                    } else send({ jsonrpc: '2.0', id: value.id, error: { code: -32601, message: 'Client method not supported by smoke test' } });
                }
                return;
            }
            const task = pending.get(value.id); if (!task) return;
            pending.delete(value.id); clearTimeout(task.timer);
            if (value.error) task.reject(new Error(summarizeError(value.error))); else task.resolve(value.result);
        });
        const init = await request('initialize', { protocolVersion: 1, clientInfo: { name: 'rpi-work-live-smoke', version: '0.1.9' }, clientCapabilities: {} });
        report.initialize = init.protocolVersion === 1;
        report.agentInfo = init.agentInfo;
        report.capabilities = init.agentCapabilities;
        report.authMethods = init.authMethods?.map(value => ({ id: value.id, name: value.name }));
        const session = await request('session/new', { cwd, mcpServers: [] }, 60000);
        sessionId = session.sessionId; report.session = typeof sessionId === 'string';
        report.config = session.configOptions?.map(value => ({ id: value.id, name: value.name, currentValue: value.currentValue, options: value.options?.length }));
        if (prompt && report.session) {
            const result = await request('session/prompt', { sessionId, prompt: [{ type: 'text', text: 'Connection smoke test only. Reply with exactly ACP_OK. Do not call tools, access files, execute commands, or change anything.' }] }, 60000);
            report.stopReason = result.stopReason; report.reply = text.slice(0, 1000);
            report.promptSucceeded = result.stopReason === 'end_turn' && text.trim() === 'ACP_OK';
        }
    } catch (error) { report.error = summarizeError(error.message); }
    finally {
        report.stderrBytes = stderrBytes;
        lines?.close();
        if (child?.pid) {
            if (process.platform === 'win32') spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
            else child.kill('SIGTERM');
        }
        for (const task of pending.values()) clearTimeout(task.timer);
        pending.clear(); reports.push(report); console.log(JSON.stringify(report, null, 2));
    }
}
fs.mkdirSync('artifacts', { recursive: true });
fs.writeFileSync(`artifacts/acp-live-${prompt ? 'prompt' : 'handshake'}${only >= 0 ? '-' + backends[0] : ''}.json`, JSON.stringify({ root, reports }, null, 2));
if (reports.some(report => !report.initialize || !report.session || (prompt && !report.promptSucceeded))) process.exitCode = 1;
