"""Isolated headless/CLI acceptance: synthetic RPC only, random loopback ports.

Build: cargo build -p work-cli -p work-ui -p work-server --bin work-cli
       --bin work-ui --bin work-server-rpc-fixture --locked
Run: python tests/e2e/verify-headless-node.py
Optional: WORK_HEADLESS_CLI, WORK_HEADLESS_UI, WORK_HEADLESS_RPC, WORK_HEADLESS_REPORT.
Never reads real node data, credentials or installed agents. No real model/tools.
"""
import ctypes
import errno
import hashlib
import json
import os
from pathlib import Path
import queue
import secrets
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
SUFFIX = '.exe' if os.name == 'nt' else ''
CLI = Path(os.environ.get('WORK_HEADLESS_CLI', ROOT / f'target/debug/work-cli{SUFFIX}')).resolve()
UI = Path(os.environ.get('WORK_HEADLESS_UI', ROOT / f'target/debug/work-ui{SUFFIX}')).resolve()
RPC = Path(os.environ.get('WORK_HEADLESS_RPC', ROOT / f'target/debug/work-server-rpc-fixture{SUFFIX}')).resolve()
REPORT = Path(os.environ.get('WORK_HEADLESS_REPORT', ROOT / 'artifacts/headless-node/e2e.json'))
checks, owners = [], []


def passed(name):
    checks.append(name)
    print('PASS', name, flush=True)


def environment(root):
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(('RPI_WORK_', 'RPI_HTTP_')) or key in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'ANTHROPIC_API_KEY', 'OPENAI_API_KEY'):
            env.pop(key, None)
    home = root / 'home'
    home.mkdir(parents=True, exist_ok=True)
    env.update(HOME=str(home), USERPROFILE=str(home), LOCALAPPDATA=str(home), XDG_DATA_HOME=str(home),
               RPI_CODING_AGENT_DIR=str(home / 'agent'), NO_PROXY='127.0.0.1,localhost',
               RPI_WORK_DATA_DIR=str(root / 'data'), RPI_WORK_RPI_BIN=str(RPC),
               RPI_WORK_HTTP_BIND='127.0.0.1', RPI_WORK_HTTP_PORT='0', RPI_WORK_HTTP_PASSWORD=secrets.token_hex(24))
    return env


def invoke(args, env, expected=0, input=None):
    p = subprocess.run([str(CLI), *args], env=env, input=input, text=True, encoding='utf-8', capture_output=True, timeout=25)
    # Diagnostics may be retained on failure, but never the test password.
    combined = p.stdout + p.stderr
    for key in ('RPI_WORK_HTTP_PASSWORD', 'RPI_WORK_PASSWORD'):
        secret = env.get(key)
        assert not secret or secret not in combined, 'credential appeared in CLI output'
    assert (p.returncode == 0) if expected == 0 else (p.returncode != 0), f'CLI unexpected exit: {p.returncode}: {p.stderr}'
    return p


def value(args, env, **kwargs):
    return json.loads(invoke(args, env, **kwargs).stdout)


class Owner:
    def __init__(self, env, ui=False):
        self.env, self.errors = env.copy(), []
        args = [str(UI), '--headless', '--shutdown-stdin'] if ui else [str(CLI), 'serve', '--shutdown-stdin']
        self.process = subprocess.Popen(args, env=self.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, encoding='utf-8')
        owners.append(self)
        lines = queue.Queue()
        def read_stdout():
            for line in self.process.stdout:
                lines.put(line)
            lines.put(None)
        def read_stderr():
            for line in self.process.stderr:
                self.errors.append(line)
        threading.Thread(target=read_stdout, daemon=True).start()
        threading.Thread(target=read_stderr, daemon=True).start()
        line = lines.get(timeout=25)
        assert line and line.endswith('\n'), 'owner exited without complete LF readiness: ' + ''.join(self.errors)
        self.info = json.loads(line)
        required = {'ready', 'address', 'nodeId', 'sharingEnabled', 'webHosted'}
        assert required <= set(self.info) <= required | {'warning'}
        if 'warning' in self.info:
            assert self.info['sharingEnabled'] is False and isinstance(self.info['warning'], str)
        assert self.info['ready'] and self.info['nodeId']
        assert self.info['address'].startswith('http://127.0.0.1:')
        assert self.env['RPI_WORK_HTTP_PASSWORD'] not in line
        self.client_env = self.env.copy()
        self.client_env.update(RPI_WORK_URL=self.info['address'], RPI_WORK_PASSWORD=self.env['RPI_WORK_HTTP_PASSWORD'])

    def stop(self):
        if self.process.poll() is None:
            self.process.stdin.close()
        code = self.process.wait(timeout=20)
        assert code == 0, f'cooperative owner stop failed: {code}: {"".join(self.errors)}'

    def request(self, path, method='GET', body=None, origin=None, host=None):
        headers = {'Content-Type': 'application/json', 'x-rpi-web': '1'}
        if origin:
            headers['Origin'] = origin
        if host:
            headers['Host'] = host
        req = urllib.request.Request(self.info['address'] + path, data=None if body is None else json.dumps(body).encode(), headers=headers, method=method)
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        try:
            with opener.open(req, timeout=10) as r:
                return r.status, dict(r.headers), r.read()
        except urllib.error.HTTPError as error:
            return error.code, dict(error.headers), error.read()


def find(obj, key):
    if isinstance(obj, dict):
        if key in obj:
            return obj[key]
        for child in obj.values():
            found = find(child, key)
            if found is not None:
                return found
    if isinstance(obj, list):
        for child in obj:
            found = find(child, key)
            if found is not None:
                return found
    return None


def running(pid):
    if os.name != 'nt':
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False
    # Query only the synthetic child observed through get_state. Never signal
    # PID 0 on Windows (os.kill(pid, 0) is not the POSIX existence probe).
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    kernel.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    handle = kernel.OpenProcess(0x100000, False, pid)
    if not handle:
        return False
    try:
        return kernel.WaitForSingleObject(handle, 0) == 258
    finally:
        kernel.CloseHandle(handle)


def set_pipe_blocking(fd, blocking):
    if os.name != 'nt':
        os.set_blocking(fd, blocking)
        return
    import msvcrt
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.SetNamedPipeHandleState.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p, ctypes.c_void_p]
    mode = wintypes.DWORD(0 if blocking else 1)
    assert kernel.SetNamedPipeHandleState(msvcrt.get_osfhandle(fd), ctypes.byref(mode), None, None)


def filled_pipe():
    read, write = os.pipe()
    set_pipe_blocking(write, False)
    size = 0
    while True:
        try:
            written = os.write(write, b'f' * 4096)
            if not written:
                break
            size += written
        except OSError as error:
            assert error.errno in (errno.EAGAIN, errno.EWOULDBLOCK, errno.ENOSPC)
            break
    assert size > 0
    set_pipe_blocking(write, True)
    return read, write, size


def drain_fd(fd):
    chunks = []
    while True:
        chunk = os.read(fd, 4096)
        if not chunk:
            return b''.join(chunks)
        chunks.append(chunk)


def backpressure(root, timeout_case, desktop=False):
    env = environment(root)
    data = Path(env['RPI_WORK_DATA_DIR'])
    data.mkdir()
    # Both streams remain full: neither startup warnings nor fatal diagnostics
    # may block supervision, cleanup, or the final nonzero exit.
    if not timeout_case:
        (data / 'sharing.json').write_text('{invalid fixture preference', encoding='utf-8')
    read, write, size = filled_pipe()
    error_read = error_write = None
    p = None
    try:
        error_read, error_write, error_size = filled_pipe()
        stderr = error_write
        args = ([str(UI), '--headless'] if desktop else [str(CLI), 'serve'])
        args += [] if timeout_case else ['--shutdown-stdin']
        started = time.monotonic()
        p = subprocess.Popen(args, env=env, stdin=subprocess.DEVNULL if timeout_case else subprocess.PIPE,
                             stdout=write, stderr=stderr)
        os.close(write)
        write = None
        if error_write is not None:
            os.close(error_write)
            error_write = None
        deadline = time.monotonic() + 8
        while not (data / 'node-id').exists() and time.monotonic() < deadline:
            assert p.poll() is None, 'backpressure probe exited before acquiring owner data'
            time.sleep(.01)
        assert (data / 'node-id').is_file()
        time.sleep(.15)
        assert p.poll() is None
        if not timeout_case:
            p.stdin.close()
        code = p.wait(timeout=22 if timeout_case else 8)
        if timeout_case:
            assert code != 0 and time.monotonic() - started >= 14
        else:
            assert code == 0
        assert drain_fd(error_read) == b'f' * error_size, 'blocked diagnostic must be dropped, not delay exit'
        assert drain_fd(read) == b'f' * size, 'late readiness written after cancellation/timeout'
    finally:
        if p is not None and p.poll() is None:
            p.kill()  # Only this test-owned process, no Agent/session was created.
            p.wait(timeout=10)
        for fd in (read, write, error_read, error_write):
            if fd is not None:
                os.close(fd)
    reopened = Owner(env)
    reopened.stop()
    label = 'desktop headless' if desktop else 'CLI serve'
    passed(f'{label}: full stdout/stderr times out and exits nonzero; stdin EOF ignored, no late ready, lock reusable' if timeout_case
           else 'full stdout/stderr cannot block supervised EOF shutdown; no late ready and owner lock reusable')


def main(root):
    for binary in (CLI, UI, RPC):
        assert binary.is_file(), f'build required: {binary.name}'
    env = environment(root / 'node-a')
    no_password = env.copy()
    no_password.pop('RPI_WORK_HTTP_PASSWORD')
    failed = invoke(['serve', '--shutdown-stdin'], no_password, expected=1, input='')
    assert not failed.stdout and not Path(env['RPI_WORK_DATA_DIR']).exists()
    passed('missing explicit headless password fails before creating node data')
    backpressure(root / 'backpressure-eof', False)
    backpressure(root / 'backpressure-deadline', True)
    backpressure(root / 'desktop-backpressure-deadline', True, desktop=True)

    a = Owner(env)
    original_id = a.info['nodeId']
    assert a.info['sharingEnabled'] and not a.info['webHosted']
    anon = a.client_env.copy()
    anon.pop('RPI_WORK_PASSWORD')
    assert value(['auth'], anon) == {'required': True, 'authenticated': False}
    invoke(['sessions'], anon, expected=1)
    wrong = a.client_env.copy()
    wrong['RPI_WORK_PASSWORD'] = 'wrong-fixture-password'
    invoke(['projects'], wrong, expected=1)
    assert value(['--password-stdin', 'auth'], wrong, input=env['RPI_WORK_HTTP_PASSWORD'] + '\n')['authenticated']
    assert value(['health'], a.client_env)
    passed('headless readiness excludes secrets; CLI env/stdin authentication rejects missing/wrong password')

    duplicate = invoke(['serve', '--shutdown-stdin'], env, expected=1, input='')
    assert not duplicate.stdout and '宿主' in duplicate.stderr
    duplicate_ui = subprocess.run([str(UI), '--headless', '--shutdown-stdin'], env=env, input='', text=True, encoding='utf-8', capture_output=True, timeout=25)
    assert duplicate_ui.returncode != 0 and not duplicate_ui.stdout
    project_dir = root / 'isolated-project'
    project_dir.mkdir()
    project = value(['project-create', 'Headless isolated project', str(project_dir)], a.client_env)
    assert project['id']
    assert find(value(['projects'], a.client_env), 'id') == project['id']
    passed('CLI and desktop headless share one data lock; original owner survives duplicate starts')

    for method in ('GET', 'OPTIONS'):
        status, headers, _ = a.request('api/v1/projects', method=method, origin='https://foreign.invalid')
        assert status == 403 and not any(k.lower() == 'access-control-allow-origin' for k in headers)
    assert a.request('api/v1/auth', host='untrusted.invalid')[0] == 403
    assert a.request('api/v1/projects')[0] == 401
    passed('real server retains exact Host/Origin and anonymous API denial, without CORS relaxation')

    busy_env = environment(root / 'bind-failure')
    busy_env['RPI_WORK_HTTP_PORT'] = str(urllib.parse.urlparse(a.info['address']).port)
    failed = invoke(['serve', '--shutdown-stdin'], busy_env, expected=1, input='')
    assert not failed.stdout
    busy_env['RPI_WORK_HTTP_PORT'] = '0'
    recovered = Owner(busy_env)
    recovered.stop()
    assert value(['health'], a.client_env)
    passed('occupied listener is never adopted; fatal HTTP startup releases its own data lock')

    b = Owner(environment(root / 'node-b'))
    assert b.info['nodeId'] != original_id
    foreign = b.client_env.copy()
    foreign['RPI_WORK_PASSWORD'] = env['RPI_WORK_HTTP_PASSWORD']
    invoke(['projects'], foreign, expected=1)
    assert value(['projects'], b.client_env) == []
    b.stop()
    passed('explicit CLI node selection has independent password, identity and project store')

    # Explicit legacy fixture only. Seed while stopped, not via a production
    # fallback create operation or a second in-process workspace owner.
    a.stop()
    session_id = 'headless-synthetic-session'
    registry = Path(env['RPI_WORK_DATA_DIR']) / 'rpc/projects' / project['id'] / 'sessions.json'
    registry.parent.mkdir(parents=True, exist_ok=True)
    registry.write_text(json.dumps({'sessions': [{'sessionId': session_id, 'projectId': project['id']}]}), encoding='utf-8')
    a = Owner(env)
    assert a.info['nodeId'] == original_id
    assert any(row['sessionId'] == session_id for row in value(['sessions'], a.client_env))
    value(['session-rename', session_id, 'Shared headless fixture title'], a.client_env)
    state = value(['state', session_id], a.client_env)
    child_pid = find(state, 'fixturePid')
    assert isinstance(child_pid, int) and running(child_pid)
    ack = value(['prompt', session_id, 'headless-owner-hold-fixture'], a.client_env)
    assert ack['success']
    after_exit = value(['state', session_id], a.client_env)
    assert find(after_exit, 'fixturePid') == child_pid and find(after_exit, 'isRunning') is True
    assert running(child_pid)
    passed('typed prompt returns ACK only; after CLI exits the same owner child retains an active synthetic run')
    assert value(['abort', session_id], a.client_env)['success']
    assert find(value(['state', session_id], a.client_env), 'isRunning') is False
    assert value(['history', session_id], a.client_env) is not None
    reply = value(['dialog-reply', session_id, 'rpc-fixture-request', 'true'], a.client_env)
    assert reply['request']['requestId'] == 'rpc-fixture-request' and reply['request']['answer'] is True
    passed('typed abort/history and RPC dialog routing work; this permissive RPC fixture does not prove ACP permission validation')

    value(['prompt', session_id, 'headless-owner-hold-fixture'], a.client_env)
    a.stop()
    deadline = time.monotonic() + 5
    while running(child_pid) and time.monotonic() < deadline:
        time.sleep(.05)
    assert not running(child_pid)
    a = Owner(env, ui=True)
    assert a.info['nodeId'] == original_id
    sessions = value(['sessions'], a.client_env)
    assert any(row.get('title') == 'Shared headless fixture title' for row in sessions)
    assert find(value(['projects'], a.client_env), 'id') == project['id']
    a.stop()
    passed('cooperative shutdown reaps active child before lock reuse; desktop --headless reopens identical data and identity')

    preference = Path(env['RPI_WORK_DATA_DIR']) / 'sharing.json'
    for saved in ('{"allow_sharing":false}', '{invalid saved preference'):
        preference.write_text(saved, encoding='utf-8')
        closed = Owner(env)
        assert closed.info['sharingEnabled'] is False
        invoke(['sessions'], closed.client_env, expected=1)
        assert preference.read_text(encoding='utf-8') == saved
        closed.stop()
    passed('saved sharing=false and corrupt preferences both fail closed and are never rewritten to enabled')


if __name__ == '__main__':
    success, failure = False, None
    try:
        with tempfile.TemporaryDirectory(prefix='rpi-headless-synthetic-') as temporary:
            try:
                main(Path(temporary))
                success = True
            finally:
                for owner in reversed(owners):
                    if owner.process.poll() is None:
                        try:
                            owner.stop()
                        except Exception:
                            owner.process.kill()  # Only the exact test-owned Popen.
                            owner.process.wait(timeout=10)
                            raise
    except Exception as error:
        failure = f'{type(error).__name__}: {error}'
        traceback.print_exc()
        print('FAIL', failure, file=sys.stderr)
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps({'success': success, 'checks': checks, 'count': len(checks), 'failure': failure,
                                 'syntheticOnly': True, 'binaries': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (CLI, UI, RPC) if p.is_file()}}, indent=2) + '\n', encoding='utf-8')
    sys.exit(0 if success else 1)
