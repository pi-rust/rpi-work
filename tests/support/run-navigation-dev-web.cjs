// Same real dev-web configuration as run-dev-web.cjs, with a portable cooperative
// stdin/EOF shutdown adapter: Node's child.kill(SIGTERM) is forced on Windows.
// No replacement HTTP server, no CORS shim, no watcher and no generated assets.
const { spawn } = require('node:child_process');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const readline = require('node:readline');

module.exports = function runNavigationDevWeb({ backendUrl, assets }) {
  const backend = new URL(backendUrl);
  if (backend.protocol !== 'http:' || backend.hostname !== '127.0.0.1' || !backend.port) {
    throw new Error('navigation proxy requires an owned loopback HTTP fixture');
  }
  const entry = pathToFileURL(path.resolve('tools/dev/dev-web.mjs')).href;
  const wrapper = `await import(${JSON.stringify(entry)}); process.stdin.resume(); process.stdin.once('end', () => process.emit('SIGTERM'));`;
  const child = spawn(process.execPath, ['--input-type=module', '-e', wrapper], {
    env: { ...process.env, PORT: '0', RPI_UI_HOST: '127.0.0.1',
      RPI_API_HOST: '127.0.0.1', RPI_API_PORT: backend.port,
      RPI_UI_ENTRY_MODE: 'hosted', RPI_UI_ALLOWED_HOSTS: '', RPI_UI_ASSETS: path.resolve(assets) },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const diagnostics = { stdout: '', stderr: '', exitCode: null, signal: null, cooperative: false };
  let exited = false, stopping, rejectReady;
  const lines = readline.createInterface({ input: child.stdout });
  child.stdout.on('data', data => { diagnostics.stdout += data; });
  child.stderr.on('data', data => { diagnostics.stderr += data; });
  child.stdin.on('error', error => { diagnostics.stdinError = error.message; rejectReady?.(error); });
  const exit = new Promise(resolve => {
    child.once('exit', (code, signal) => {
      exited = true; diagnostics.exitCode = code; diagnostics.signal = signal;
      rejectReady?.(new Error(`dev-web exited ${code}`)); resolve();
    });
    child.once('error', error => {
      exited = true; diagnostics.error = error.message; rejectReady?.(error); resolve();
    });
  });
  const ready = new Promise((resolve, reject) => {
    rejectReady = reject;
    lines.on('line', line => {
      const match = /^\[dev-web\] local URL: (http:\/\/127\.0\.0\.1:(\d+)\/)\s*$/.exec(line);
      if (match && Number(match[2]) > 0) resolve(match[1]);
    });
  });
  ready.catch(() => {});
  return { ready, diagnostics,
    stop() {
      return stopping ||= (async () => {
        let timer, forced = false;
        try {
          if (!exited) {
            child.stdin.end(); // Real dev-web SIGTERM handler drains its sockets.
            timer = setTimeout(() => { forced = true; child.kill('SIGKILL'); }, 10000);
          }
          await exit;
          diagnostics.cooperative = !forced && diagnostics.exitCode === 0;
          if (!diagnostics.cooperative) throw new Error('dev-web did not cooperatively shut down');
        } finally { clearTimeout(timer); lines.close(); }
      })();
    },
  };
};
