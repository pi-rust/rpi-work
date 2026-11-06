"""Verify cross-language component release contracts without publishing anything.

Platform file bytes are isolated fixtures. This test does not claim that native
code has been built or executed on all six platforms.
"""
from pathlib import Path
import importlib.util
import json
import subprocess
import tempfile

repository = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("component_assets", repository / "scripts/ci/create-component-assets.py")
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)


def export(*arguments):
    run = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "rpi-http-component", "--bin", "export-component", "--", *arguments], cwd=repository, capture_output=True, text=True, encoding="utf-8", check=True)
    return json.loads(run.stdout)


template = export()
with tempfile.TemporaryDirectory(prefix="rpi-component-release-contract-") as temporary:
    root, output = Path(temporary) / "input", Path(temporary) / "output"
    root.mkdir()
    for target, suffix in assets.TARGETS.items():
        (root / f"rpi-component-ui-{target}.json").write_text(json.dumps(template, ensure_ascii=False), encoding="utf-8")
        (root / f"rpi-http-native-{target}{suffix}").write_bytes(b"isolated-native-archive-fixture-" + target.encode())
    path = assets.build(root, output, "pi-rust/rpi-work", "v0.1.9")
    package = json.loads(path.read_text(encoding="utf-8"))
    descriptor = Path(temporary) / "runtime.json"
    descriptor.write_text(json.dumps(package["manifest"]["runtime"]), encoding="utf-8")
    assert export(str(descriptor)) == package, "The release must implement the exact Rust Bundle contract and digest"
    (root / "rpi-http-native-x86_64-pc-windows-msvc.dll").unlink()
    try:
        assets.build(root, output, "pi-rust/rpi-work", "v0.1.9")
    except ValueError:
        pass
    else:
        raise AssertionError("Missing native targets must stop release assembly")
print("PASS component release target identities, artifact hashes, exact Rust Bundle compatibility and missing-target rejection (isolated platform fixtures)")
