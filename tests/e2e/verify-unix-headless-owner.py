"""Linux full CLI/HTTP owner lifecycle acceptance, synthetic RPC only.
No real model, no SessionCreate, no real user data or credentials.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import secrets
import signal
import subprocess
import sys
import tempfile
import threading
import time


class Verification:
    def __init__(self, cli, rpc):
        self.cli, self.rpc = cli, rpc
        self.checks, self.owners, self.children = [], [], []
        self.forced_cleanup = False

    def passed(self, label):
        self.checks.append(label)
        print('PASS ' + label, flush=True)

    def environment(self, root):
        home = root / 'home'
        home.mkdir()
        return {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'USERPROFILE': str(home),
                'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_DATA_HOME': str(home / 'data'),
                'RPI_CODING_AGENT_DIR': str(home / 'agent'), 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8',
                'NO_PROXY': '127.0.0.1,localhost', 'RPI_WORK_DATA_DIR': str(root / 'data'),
                'RPI_WORK_RPI_BIN': str(self.rpc), 'RPI_WORK_HTTP_BIND': '127.0.0.1',
                'RPI_WORK_HTTP_PORT': '0', 'RPI_WORK_HTTP_PASSWORD': secrets.token_hex(24)}

    def value(self, args, owner):
        env = owner.env.copy()
        env.update(RPI_WORK_URL=owner.info['address'], RPI_WORK_PASSWORD=env['RPI_WORK_HTTP_PASSWORD'])
        p = subprocess.run([str(self.cli), *args], env=env, capture_output=True, timeout=10)
        assert p.returncode == 0
        assert env['RPI_WORK_HTTP_PASSWORD'].encode() not in p.stdout + p.stderr
        return json.loads(p.stdout)

    def child(self, owner, session):
        state = self.value(['state', session], owner)
        pid = find(state, 'fixturePid')
        assert isinstance(pid, int) and pid > 1 and Path(f'/proc/{pid}').exists()
        context = find(state, 'context')
        assert isinstance(context, dict) and context.get('fixturePid') == pid
        if pid not in self.children:
            self.children.append(pid)
        return pid

    def main(self, root):
        env = self.environment(root)
        a = Owner(self, env, shutdown_stdin=False)
        identity = a.info['nodeId']
        # EOF is present from process startup, without the opt-in switch.
        time.sleep(.4)
        assert a.process.poll() is None
        assert self.value(['health'], a)
        self.passed('real serve emits LF Ready on random loopback port; default stdin EOF ignored')
        project_path = root / 'project'
        project_path.mkdir()
        project = self.value(['project-create', 'Unix synthetic owner', str(project_path)], a)
        assert isinstance(project['id'], str)
        a.sigterm()
        self.passed('actual SIGTERM exits successfully without forced termination')

        # Explicit legacy fixture seed, only after the sole owner has stopped.
        session = 'unix-owner-synthetic-session'
        registry = Path(env['RPI_WORK_DATA_DIR']) / 'rpc/projects' / project['id'] / 'sessions.json'
        registry.parent.mkdir(parents=True, exist_ok=True)
        registry.write_text(json.dumps({'sessions': [{'sessionId': session, 'projectId': project['id']}]}))
        b = Owner(self, env, shutdown_stdin=False)
        assert b.info['nodeId'] == identity
        assert any(row['sessionId'] == session for row in self.value(['sessions'], b))
        pid = self.child(b, session)
        assert self.value(['prompt', session, 'headless-owner-hold-fixture'], b)['success']
        state = self.value(['state', session], b)
        assert find(state, 'fixturePid') == pid and find(state, 'isRunning') is True
        assert Path(f'/proc/{pid}').exists()
        self.passed('stopped-only legacy seed loads real fixture child; CLI prompt leaves synthetic run active')
        b.sigterm()
        deadline = time.monotonic() + 5
        while Path(f'/proc/{pid}').exists() and time.monotonic() < deadline:
            time.sleep(.05)
        assert not Path(f'/proc/{pid}').exists()
        self.passed('SIGTERM normally reaps active RPC fixture child before returning success')

        c = Owner(self, env, shutdown_stdin=True)
        assert c.info['nodeId'] == identity
        assert any(row['id'] == project['id'] for row in self.value(['projects'], c))
        assert any(row['sessionId'] == session for row in self.value(['sessions'], c))
        second_pid = self.child(c, session)
        assert self.value(['prompt', session, 'headless-owner-hold-fixture'], c)['success']
        c.eof()
        deadline = time.monotonic() + 5
        while Path(f'/proc/{second_pid}').exists() and time.monotonic() < deadline:
            time.sleep(.05)
        assert not Path(f'/proc/{second_pid}').exists()
        self.passed('opt-in --shutdown-stdin EOF exits zero and reaps active fixture child')
        d = Owner(self, env, shutdown_stdin=False)
        assert d.info['nodeId'] == identity
        assert self.value(['health'], d)
        d.sigterm()
        self.passed('same data lock reusable and stable nodeId after SIGTERM and opt-in EOF shutdown')

    def cleanup(self):
        for owner in reversed(self.owners):
            if owner.process.poll() is None:
                self.forced_cleanup = True
                owner.process.send_signal(signal.SIGTERM)
                try:
                    owner.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    owner.process.kill()
                    owner.process.wait(timeout=5)
        # Never turn forced cleanup into a passing normal-SIGTERM test.
        # Fixture PIDs are observed only, never blindly signalled after PID reuse.


class Owner:
    def __init__(self, verifier, env, shutdown_stdin):
        self.env = env.copy()
        command = [str(verifier.cli), 'serve']
        if shutdown_stdin:
            command.append('--shutdown-stdin')
        # Inherit verifier's isolated group so build driver timeout can clean up
        # verifier + owners + fixture descendants together.
        self.process = subprocess.Popen(command, env=self.env,
                                        stdin=subprocess.PIPE if shutdown_stdin else subprocess.DEVNULL,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        verifier.owners.append(self)
        lines = queue.Queue()
        errors = []

        def stdout():
            for line in self.process.stdout:
                lines.put(line)
            lines.put(None)

        def stderr():
            for line in self.process.stderr:
                # Drain without retaining possibly sensitive diagnostics.
                errors.append(len(line))

        threading.Thread(target=stdout, daemon=True).start()
        threading.Thread(target=stderr, daemon=True).start()
        line = lines.get(timeout=15)
        assert line and line.endswith(b'\n')
        assert env['RPI_WORK_HTTP_PASSWORD'].encode() not in line
        self.info = json.loads(line)
        assert self.info['ready'] is True and self.info['nodeId']
        assert self.info['sharingEnabled'] is True and self.info['webHosted'] is False
        assert self.info['address'].startswith('http://127.0.0.1:')
        assert not self.info['address'].endswith(':0/') and not self.info['address'].endswith(':0')

    def sigterm(self):
        assert self.process.poll() is None
        self.process.send_signal(signal.SIGTERM)
        assert self.process.wait(timeout=15) == 0

    def eof(self):
        assert self.process.poll() is None
        self.process.stdin.close()
        assert self.process.wait(timeout=15) == 0


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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--rpc', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    verifier = Verification(args.cli, args.rpc)
    result = {'success': False, 'syntheticOnly': True, 'realModel': False,
              'sessionCreateUsed': False, 'fullOwnerNotPureModule': True}
    try:
        assert sys.platform == 'linux'
        with tempfile.TemporaryDirectory(prefix='unix-owner-acceptance-') as temporary:
            try:
                verifier.main(Path(temporary))
            finally:
                verifier.cleanup()
        assert not verifier.forced_cleanup
        result['success'] = True
    except Exception as error:
        result['errorClass'] = type(error).__name__
    result.update(checks=verifier.checks, forcedCleanup=verifier.forced_cleanup,
                  fixturePids=verifier.children,
                  binaries={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (args.cli, args.rpc)},
                  scriptSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, indent=2) + '\n')
    return 0 if result['success'] else 1


if __name__ == '__main__':
    sys.exit(main())
