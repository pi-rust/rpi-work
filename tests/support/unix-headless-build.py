"""Linux offline full-workspace owner build; all Cargo cache writes stay in /tmp.
Run via WSL --exec (no login shell). Never downloads or changes shared caches.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]
REPORT = ROOT / 'artifacts/headless-node/unix-owner'
TOOLCHAIN = Path('/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin')
WINDOWS_CARGO = Path('/mnt/c/Users/bigfish/.cargo')
REV = '17ba89c0098f018db76e22051fe8c73e2792dc1c'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def production_hash():
    paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock']
    for base in ('apps', 'crates'):
        paths += [p for p in (ROOT / base).rglob('*') if p.is_file()
                  and (p.suffix == '.rs' or p.name in ('Cargo.toml', 'Cargo.lock'))
                  and not {'target', 'node_modules', '.git'}.intersection(p.relative_to(ROOT).parts)]
    digest = hashlib.sha256()
    for p in sorted(paths):
        digest.update(str(p.relative_to(ROOT)).encode())
        digest.update(p.read_bytes())
    return digest.hexdigest()


def merge_registry(destination, packages, result):
    registries = [Path('/root/.cargo/registry'), WINDOWS_CARGO / 'registry']
    assert any((WINDOWS_CARGO / 'registry/index').glob('*/.cache/di/ox/dioxus'))
    index_count = cache_count = source_count = 0
    # Preserve both index hashes. Overlay Windows entries after Linux entries,
    # but copy only locked package names, not the 150GB source registry.
    for registry in registries:
        for index in (registry / 'index').glob('*'):
            out = destination / 'index' / index.name
            out.mkdir(parents=True, exist_ok=True)
            if (index / 'config.json').exists():
                shutil.copy2(index / 'config.json', out / 'config.json')
            for package in packages:
                name = package['name'].lower()
                key = name if len(name) == 1 else ('2/' + name if len(name) == 2 else
                      ('3/' + name[0] + '/' + name if len(name) == 3 else name[:2] + '/' + name[2:4] + '/' + name))
                entry = index / '.cache' / key
                if entry.is_file():
                    target = out / '.cache' / key
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(entry, target)
                    index_count += 1
    # Archive copies are private; cargo extracts only what its Linux build needs.
    for registry in registries:
        for index in (registry / 'index').glob('*'):
            for package in packages:
                name = package['name'] + '-' + package['version']
                archive = registry / 'cache' / index.name / (name + '.crate')
                target = destination / 'cache' / index.name / archive.name
                if archive.is_file() and not target.exists():
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(archive, target)
                    cache_count += 1
                elif not target.exists():
                    source = registry / 'src' / index.name / name
                    target_source = destination / 'src' / index.name / name
                    if source.is_dir() and not target_source.exists():
                        shutil.copytree(source, target_source)
                        source_count += 1
    result['cacheMerge'] = {'writeOnlyTemp': True, 'sharedRegistrySymlink': False,
                            'indexOrder': ['linux', 'windows-overlay'], 'indexEntriesCopied': index_count,
                            'archivesCopied': cache_count, 'sourceDirectoriesCopied': source_count}


def main():
    assert sys.platform == 'linux'
    parser = argparse.ArgumentParser()
    parser.add_argument('--budget', type=int, default=740)
    args = parser.parse_args()
    REPORT.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    deadline = started + min(args.budget, 900)
    result = {'platform': sys.platform, 'offline': True, 'locked': True,
              'workspaceReduced': False, 'status': 'blocked', 'commands': [],
              'ownerAcceptanceExecuted': False, 'cleanup': False,
              'scriptSha256': sha(Path(__file__)), 'rootLockSha256Before': sha(ROOT / 'Cargo.lock'),
              'rootProductionHashBefore': production_hash()}
    code = 2
    with tempfile.TemporaryDirectory(prefix='headless-unix-owner-') as temporary:
        temp = Path(temporary)
        source, home, cargo_home, target = [temp / n for n in ('source', 'home', 'cargo-home', 'target')]
        for p in (source, home, cargo_home, target):
            p.mkdir()
        try:
            lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
            packages = [p for p in lock['package'] if p.get('source', '').startswith('registry+')]
            merge_registry(cargo_home / 'registry', packages, result)
            db = WINDOWS_CARGO / 'git/db/rpi-package-b3428740bcf0e285'
            probe = subprocess.run(['git', '--git-dir=' + str(db), 'cat-file', '-t', REV], capture_output=True, timeout=15)
            assert probe.returncode == 0 and probe.stdout.strip() == b'commit'
            for relative in ('db/rpi-package-b3428740bcf0e285', 'checkouts/rpi-package-b3428740bcf0e285'):
                shutil.copytree(WINDOWS_CARGO / 'git' / relative, cargo_home / 'git' / relative)
            result['gitCacheIndependentCopy'] = True
            for name in ('Cargo.toml', 'Cargo.lock'):
                shutil.copy2(ROOT / name, source / name)
            members = tomllib.loads((source / 'Cargo.toml').read_text())['workspace']['members']
            result['members'] = members
            for member in members:
                shutil.copytree(ROOT / member, source / member,
                                ignore=shutil.ignore_patterns('target', 'node_modules', '.env', '.env.*', '.git', '__pycache__'))
            shutil.copytree(ROOT / 'assets/packages', source / 'assets/packages')
            result['lockSha256Before'] = sha(source / 'Cargo.lock')
            env = {'PATH': str(TOOLCHAIN) + ':/usr/bin:/bin', 'HOME': str(home),
                   'CARGO_HOME': str(cargo_home), 'RUSTUP_HOME': '/root/.rustup',
                   'CARGO_TARGET_DIR': str(target), 'CARGO_NET_OFFLINE': 'true',
                   'CARGO_BUILD_JOBS': '4', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8',
                   'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_DATA_HOME': str(home / 'data')}
            result['rustc'] = subprocess.check_output([str(TOOLCHAIN / 'rustc'), '--version'], env=env, text=True).strip()

            def run(command, log_name, limit):
                result['commands'].append(command)
                remaining = max(1, min(limit, deadline - time.monotonic()))
                with (REPORT / log_name).open('wb') as log:
                    p = subprocess.Popen(command, cwd=source, env=env, stdout=log,
                                         stderr=subprocess.STDOUT, start_new_session=True)
                    try:
                        returncode = p.wait(timeout=remaining)
                    except BaseException:
                        # Kill only this invocation's new process group, including
                        # rustc/linker children, before TemporaryDirectory cleanup.
                        os.killpg(p.pid, signal.SIGKILL)
                        p.wait(timeout=15)
                        raise
                if returncode:
                    text = (REPORT / log_name).read_text(errors='replace')
                    missing = re.search(r'no matching package named [`\x27]([^`\x27]+)', text)
                    archive = re.search(r'failed to download `([^`]+)`', text)
                    result['failure'] = ('missing offline package: ' + missing.group(1) if missing else
                                         'missing offline archive: ' + archive.group(1) if archive else
                                         'command failed; see ' + log_name)
                return returncode == 0

            if run(['cargo', 'build', '--offline', '--locked', '-p', 'work-cli', '-p', 'work-node-service',
                    '--bin', 'work-cli', '--bin', 'work-server-rpc-fixture'], 'build.log', 650):
                result['buildPassed'] = True
                verifier = ROOT / 'tests/e2e/verify-unix-headless-owner.py'
                assert verifier.is_file()
                result['ownerAcceptanceExecuted'] = True
                result['ownerScriptSha256'] = sha(verifier)
                owner_ok = run(['python3', str(verifier), '--cli', str(target / 'debug/work-cli'),
                                '--rpc', str(target / 'debug/work-server-rpc-fixture'),
                                '--report', str(REPORT / 'owner-report.json')], 'owner.log', 100)
                if owner_ok:
                    tests_ok = run(['cargo', 'test', '--offline', '--locked', '-p', 'work-cli',
                                    '--test', 'environment'], 'environment.log', 90)
                    result['environmentTestsPassed'] = tests_ok
                    if tests_ok:
                        result['status'] = 'pass'
                        code = 0
        except Exception as error:
            result['errorClass'] = type(error).__name__
        finally:
            if (source / 'Cargo.lock').exists():
                result['lockSha256After'] = sha(source / 'Cargo.lock')
                result['lockUnchanged'] = result.get('lockSha256Before') == result['lockSha256After']
            result['temporaryPath'] = str(temp)
    result['cleanup'] = not temp.exists()
    result['rootLockSha256After'] = sha(ROOT / 'Cargo.lock')
    result['rootLockUnchanged'] = result['rootLockSha256Before'] == result['rootLockSha256After']
    result['rootProductionHashAfter'] = production_hash()
    result['rootProductionUnchanged'] = result['rootProductionHashBefore'] == result['rootProductionHashAfter']
    result['elapsedSeconds'] = round(time.monotonic() - started, 2)
    (REPORT / 'build-report.json').write_text(json.dumps(result, indent=2) + '\n')
    return code


if __name__ == '__main__':
    sys.exit(main())
