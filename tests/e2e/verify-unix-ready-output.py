"""Linux-only validation of the exact production ready_output module, not the node owner.

Run in WSL/Linux: python3 tests/e2e/verify-unix-ready-output.py
Uses existing offline registry cache and an existing toolchain, a temporary Cargo
project/target/home, and owned pipes/processes only. No root Cargo.lock changes,
network, installed rpi, model/tools, user sessions or service shutdowns.
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'crates/host/work-node-host/src/ready_output.rs'
REPORT = ROOT / 'artifacts/headless-node/unix-ready-output'
checks = []


def passed(name):
    checks.append(name)
    print('PASS', name, flush=True)


def full_pipe():
    read, write = os.pipe()
    os.set_blocking(write, False)
    size = 0
    while True:
        try:
            size += os.write(write, b'f' * 4096)
        except BlockingIOError:
            break
    os.set_blocking(write, True)
    return read, write, size


def drain(fd):
    result = b''
    while True:
        data = os.read(fd, 8192)
        if not data:
            return result
        result += data


def pipe_case(binary, env, base, timeout=False):
    import fcntl
    out_r, out_w, out_size = full_pipe()
    err_r, err_w, err_size = full_pipe()
    flags = [fcntl.fcntl(fd, fcntl.F_GETFL) for fd in (out_w, err_w)]
    marker = base / ('timeout-marker' if timeout else 'signal-marker')
    p = None
    try:
        started = time.monotonic()
        p = subprocess.Popen([str(binary), str(marker)], env=env, stdin=subprocess.DEVNULL,
                             stdout=out_w, stderr=err_w)
        deadline = time.monotonic() + 5
        while not marker.exists() and time.monotonic() < deadline:
            assert p.poll() is None
            time.sleep(.01)
        assert marker.exists()
        time.sleep(.1)  # Allow the native writer to encounter the full pipe.
        assert p.poll() is None
        if not timeout:
            p.send_signal(signal.SIGTERM)  # Only this test-created process.
        code = p.wait(timeout=22 if timeout else 5)
        assert code == (1 if timeout else 0)
        if timeout:
            assert time.monotonic() - started >= 14
        assert [fcntl.fcntl(fd, fcntl.F_GETFL) for fd in (out_w, err_w)] == flags
        os.close(out_w)
        out_w = None
        os.close(err_w)
        err_w = None
        assert drain(out_r) == b'f' * out_size
        assert drain(err_r) == b'f' * err_size
    finally:
        if p is not None and p.poll() is None:
            p.kill()
            p.wait(timeout=5)
        for fd in (out_r, out_w, err_r, err_w):
            if fd is not None:
                os.close(fd)
    passed('double-full pipes: deadline/nonzero exit, flags restored, no late ready or diagnostic' if timeout
           else 'real SIGTERM cancels double-full pipes, exits successfully, restores flags, no late ready')


def main():
    assert os.name == 'posix' and Path('/proc').is_dir(), 'Linux/WSL required'
    original_home = Path.home()
    toolchain = Path(os.environ.get('WORK_UNIX_TOOLCHAIN', original_home / '.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin'))
    cache = Path(os.environ.get('WORK_UNIX_CACHE', original_home / '.cargo'))
    REPORT.mkdir(parents=True, exist_ok=True)
    result = {'scope': 'Exact production ready_output.rs in a temporary two-dependency crate; not full work-host/work-cli or owner cleanup acceptance',
              'passed': False, 'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
              'checks': checks, 'noNetwork': True, 'noModelsToolsOrUserData': True}
    try:
        with tempfile.TemporaryDirectory(prefix='rpi-work-unix-output-') as directory:
            base = Path(directory)
            cargo_home = base / 'cargo'
            (cargo_home / 'registry').mkdir(parents=True)
            for component in ('src', 'cache', 'index'):
                source = cache / 'registry' / component
                assert source.is_dir(), 'Existing offline registry cache required'
                (cargo_home / 'registry' / component).symlink_to(source, target_is_directory=True)
            (base / 'home').mkdir()
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith(('RPI_', 'WORK_', 'CARGO_', 'RUST_', 'HTTP_', 'HTTPS_', 'ALL_PROXY'))
                   and not any(word in key.upper() for word in ('API_KEY', 'TOKEN', 'SECRET', 'PASSWORD', 'CREDENTIAL'))}
            env.update(HOME=str(base / 'home'), CARGO_HOME=str(cargo_home), CARGO_TARGET_DIR=str(base / 'target'),
                       PATH=f'{toolchain}:/usr/bin:/bin', RUSTC=str(toolchain / 'rustc'), NO_PROXY='*')
            locked = tomllib.loads((ROOT / 'Cargo.lock').read_text())
            versions = {p['name']: p['version'] for p in locked['package'] if p['name'] in ('libc', 'tokio')}
            result['dependencyVersionsFromRootLock'] = versions
            manifest = base / 'Cargo.toml'
            manifest.write_text('''[package]
name = "work-ready-output-unix-check"
version = "0.0.0"
edition = "2024"
[dependencies]
libc = "0.2"
tokio = { version = "1", features = ["rt", "macros", "signal", "time", "sync"] }
[lints.rust]
unsafe_code = "warn"
[lints.clippy]
all = "warn"
pedantic = "warn"
'''.replace('libc = "0.2"', f'libc = "={versions["libc"]}"')
                .replace('version = "1", features', f'version = "={versions["tokio"]}", features'))
            (base / 'src').mkdir()
            (base / 'src/main.rs').write_text(f'''#[path = {json.dumps(str(SOURCE))}]
mod ready_output;
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {{
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    std::fs::write(std::env::args_os().nth(1).unwrap(), b"signal-registered").unwrap();
    let stop = async {{ term.recv().await; Ok(()) }};
    tokio::pin!(stop);
    match ready_output::publish_and_wait(b"{{\\\"ready\\\":true}}\\n", stop.as_mut()).await {{
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {{ ready_output::report_error(&error); std::process::ExitCode::FAILURE }}
    }}
}}
''')
            result['rustc'] = subprocess.check_output([str(toolchain / 'rustc'), '--version'], env=env, text=True).strip()
            for action in ('test', 'build'):
                command = [str(toolchain / 'cargo'), action, '--offline', '--manifest-path', str(manifest)]
                if action == 'build':
                    command.append('--locked')
                with (REPORT / f'{action}.log').open('w') as log:
                    finished = subprocess.run(command, env=env, cwd=base, stdout=log, stderr=subprocess.STDOUT, timeout=180)
                assert finished.returncode == 0, 'Offline module build/test failed; inspect scoped log'
                passed(f'exact production Unix module: cargo {action} offline')
            assert (toolchain / 'cargo-clippy').is_file(), 'Strict acceptance requires existing Clippy'
            with (REPORT / 'clippy.log').open('w') as log:
                finished = subprocess.run([str(toolchain / 'cargo'), 'clippy', '--offline', '--locked',
                                           '--all-targets', '--manifest-path', str(manifest), '--', '-D', 'warnings'],
                                          env=env, cwd=base, stdout=log, stderr=subprocess.STDOUT, timeout=180)
            assert finished.returncode == 0, 'Unix module Clippy failed; inspect scoped log'
            passed('exact production Unix module: strict offline all-targets Clippy')
            binary = base / 'target/debug/work-ready-output-unix-check'
            import select
            p = subprocess.Popen([str(binary), str(base / 'published-marker')], env=env,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                assert select.select([p.stdout], [], [], 5)[0], 'Ready frame deadline'
                line = p.stdout.readline()
                assert line.endswith(b'\n') and json.loads(line) == {'ready': True}
                p.send_signal(signal.SIGTERM)
                tail, error = p.communicate(timeout=5)
                assert p.returncode == 0 and not tail and not error
            finally:
                if p.poll() is None:
                    p.kill()
                    p.wait(timeout=5)
            passed('complete LF Ready then real SIGTERM ends the same supervised service wait')
            pipe_case(binary, env, base)
            pipe_case(binary, env, base, timeout=True)
            # A closed reader is an output error, not SIGPIPE termination or Ready.
            read, write = os.pipe()
            os.close(read)
            try:
                p = subprocess.Popen([str(binary), str(base / 'broken-marker')], env=env,
                                     stdin=subprocess.DEVNULL, stdout=write, stderr=subprocess.PIPE)
                try:
                    error = p.communicate(timeout=5)[1]
                    assert p.returncode == 1 and b'Unable to write ready status' in error
                finally:
                    if p.poll() is None:
                        p.kill()
                        p.wait(timeout=5)
            finally:
                os.close(write)
            passed('closed stdout returns nonzero sanitized error, not SIGPIPE death')
            assert len(checks) == len(set(checks)) == 7, 'Incomplete acceptance check set'
            result['expectedCheckCount'] = 7
            result['passed'] = True
    except Exception as error:
        result['exceptionName'] = type(error).__name__
        raise
    finally:
        result['scriptSha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        (REPORT / 'report.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
