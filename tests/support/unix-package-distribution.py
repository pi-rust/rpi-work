"""Offline Linux Package regression with private source/home/Cargo cache/target.
Reuses the existing cache-copy helper, never downloads or mutates shared caches.
Run: wsl --distribution Ubuntu24 --exec python3 /mnt/d/Projects/rpi-work/tests/support/unix-package-distribution.py
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
REPORT = ROOT / 'artifacts/curated-packages/unix'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    assert sys.platform == 'linux'
    helper = ROOT / 'tests/support/unix-headless-build.py'
    spec = importlib.util.spec_from_file_location('private_cache', helper)
    cache = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cache)
    REPORT.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    result = {'status': 'blocked', 'offline': True, 'locked': True, 'workspaceReduced': False,
              'realHostOrModel': False, 'scriptSha256': sha(Path(__file__)),
              'cacheHelperSha256': sha(helper), 'rootLockBefore': sha(ROOT / 'Cargo.lock'),
              'rootProductionBefore': cache.production_hash()}
    code = 2
    with tempfile.TemporaryDirectory(prefix='curated-package-unix-') as temporary:
        temp = Path(temporary)
        source, home, cargo_home, target = [temp / n for n in ('source', 'home', 'cargo-home', 'target')]
        for p in (source, home, cargo_home, target):
            p.mkdir()
        try:
            lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
            cache.merge_registry(cargo_home / 'registry', [p for p in lock['package'] if p.get('source', '').startswith('registry+')], result)
            for relative in ('db/rpi-package-b3428740bcf0e285', 'checkouts/rpi-package-b3428740bcf0e285'):
                shutil.copytree(cache.WINDOWS_CARGO / 'git' / relative, cargo_home / 'git' / relative)
            for name in ('Cargo.toml', 'Cargo.lock'):
                shutil.copy2(ROOT / name, source / name)
            members = tomllib.loads((source / 'Cargo.toml').read_text())['workspace']['members']
            result['members'] = members
            for member in members:
                shutil.copytree(ROOT / member, source / member, ignore=shutil.ignore_patterns('target', 'node_modules', '.env', '.env.*', '.git', '__pycache__'))
            shutil.copytree(ROOT / 'assets/packages', source / 'assets/packages')
            env = {'PATH': str(cache.TOOLCHAIN) + ':/usr/bin:/bin', 'HOME': str(home), 'CARGO_HOME': str(cargo_home),
                   'RUSTUP_HOME': '/root/.rustup', 'CARGO_TARGET_DIR': str(target), 'CARGO_NET_OFFLINE': 'true',
                   'CARGO_BUILD_JOBS': '4', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8',
                   'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_DATA_HOME': str(home / 'data')}
            result['rustc'] = subprocess.check_output([str(cache.TOOLCHAIN / 'rustc'), '--version'], env=env, text=True).strip()
            command = ['cargo', 'test', '--offline', '--locked', '-p', 'work-node', '--lib', 'workspace::packages']
            result['command'] = command
            with (REPORT / 'tests.log').open('wb') as log:
                process = subprocess.Popen(command, cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    returncode = process.wait(timeout=650)
                except BaseException:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=15)
                    raise
            result['returncode'] = returncode
            result['copiedLockUnchanged'] = sha(source / 'Cargo.lock') == result['rootLockBefore']
            result['status'] = 'passed' if returncode == 0 else 'failed'
            code = 0 if returncode == 0 else 1
        except Exception as error:
            result['errorClass'] = type(error).__name__
        result['temporaryPath'] = str(temp)
    result.update(cleanup=not temp.exists(), rootLockAfter=sha(ROOT / 'Cargo.lock'),
                  rootProductionAfter=cache.production_hash(), elapsedSeconds=round(time.monotonic() - started, 2))
    result['rootLockUnchanged'] = result['rootLockBefore'] == result['rootLockAfter']
    result['rootProductionUnchanged'] = result['rootProductionBefore'] == result['rootProductionAfter']
    (REPORT / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return code


if __name__ == '__main__':
    sys.exit(main())
