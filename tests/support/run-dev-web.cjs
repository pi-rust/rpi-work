// Own the real dev-web process without enabling its build watcher.
const { spawn } = require('node:child_process');
const path = require('node:path');
const readline = require('node:readline');

module.exports = function runDevWeb({ backendUrl, assets, entryMode = 'hosted' }) {
  const backend = new URL(backendUrl);
  if (backend.protocol !== 'http:' || backend.hostname !== '127.0.0.1' || !backend.port) {
    throw new Error('dev-web test helper requires an owned loopback HTTP fixture');
  }
  const child = spawn(process.execPath, [path.resolve('tools/dev/dev-web.mjs')], {
    env: {
      ...process.env,
      PORT: '0',
      RPI_UI_HOST: '127.0.0.1',
      RPI_API_HOST: '127.0.0.1',
      RPI_API_PORT: backend.port,
      RPI_UI_ENTRY_MODE: entryMode,
      RPI_UI_ALLOWED_HOSTS: '',
      RPI_UI_ASSETS: path.resolve(assets),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const diagnostics = { stdout: '', stderr: '', exitCode: null, signal: null };
  child.stdout.on('data', data => { diagnostics.stdout += data; });
  child.stderr.on('data', data => { diagnostics.stderr += data; });
  let exited = false, stopping;
  const exit = new Promise(resolve => {
    child.once('exit', (code, signal) => {
      exited = true;
      diagnostics.exitCode = code;
      diagnostics.signal = signal;
      resolve();
    });
    child.once('error', error => { exited = true; diagnostics.error = error.message; resolve(); });
  });
  const ready = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', code => reject(new Error(`dev-web exited ${code}: ${diagnostics.stderr}`)));
    readline.createInterface({ input: child.stdout }).on('line', line => {
      const match = /^\[dev-web\] local URL: (http:\/\/127\.0\.0\.1:(\d+)\/)\s*$/.exec(line);
      if (match && Number(match[2]) > 0) resolve(match[1]);
    });
  });
  return {
    ready,
    diagnostics,
    stop() {
      return stopping ||= (async () => {
        if (exited) return;
        child.kill();
        const timer = setTimeout(() => { if (!exited) child.kill('SIGKILL'); }, 3000);
        try { await exit; } finally { clearTimeout(timer); }
      })();
    },
  };
};
