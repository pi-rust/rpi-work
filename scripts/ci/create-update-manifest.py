"""Collect exactly one package per platform and sign the bytes clients verify."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib

TARGETS = {
    "x86_64-pc-windows-msvc": "zip",
    "aarch64-pc-windows-msvc": "zip",
    "x86_64-apple-darwin": "tar.gz",
    "aarch64-apple-darwin": "tar.gz",
    "x86_64-unknown-linux-gnu": "tar.gz",
    "aarch64-unknown-linux-gnu": "tar.gz",
    "aarch64-linux-android": "apk",
    "wasm32-unknown-unknown": "zip",
}


def create(root: Path, tag: str, notes: str, key_pem: str, output: Path) -> dict:
    repo = Path(__file__).resolve().parents[2]
    version = tomllib.loads((repo / "apps/work-ui/Cargo.toml").read_text())["package"]["version"]
    if tag != f"v{version}":
        raise ValueError("Release tag must match the work-ui package version")
    output.mkdir(parents=True, exist_ok=True)
    artifacts = {}
    import shutil
    for target, extension in TARGETS.items():
        name = f"rpi-work-{version}-{target}.{extension}"
        matches = list(root.rglob(name))
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one {name}, found {len(matches)}")
        source = matches[0]
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if source.with_name(name + ".sha256").read_text().split()[0] != digest:
            raise ValueError(f"Checksum mismatch: {name}")
        size = source.stat().st_size
        if not 0 < size <= 256 * 1024 * 1024:
            raise ValueError(f"Invalid package size: {name}")
        artifacts[target] = {"name": name, "size": size, "sha256": digest}
        shutil.copyfile(source, output / name)
        (output / (name + ".sha256")).write_text(f"{digest}  {name}\n", encoding="ascii")
    manifest = {"schema": 1, "version": version, "notes": notes, "artifacts": artifacts}
    document = output / "update.json"
    document.write_bytes((json.dumps(manifest, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode())
    with tempfile.TemporaryDirectory(prefix="rpi-update-sign-") as tmp:
        private = Path(tmp) / "private.pem"
        private.write_text(key_pem, encoding="ascii")
        private.chmod(0o600)
        public = subprocess.run(["openssl", "pkey", "-in", str(private), "-pubout", "-outform", "DER"], check=True, capture_output=True).stdout
        expected = base64.b64decode((repo / "crates/work-update/update-public-key.txt").read_text().strip(), validate=True)
        if public[:12] != bytes.fromhex("302a300506032b6570032100") or public[12:] != expected:
            raise ValueError("Signing key does not match the public key embedded in the client")
        signature = Path(tmp) / "signature.bin"
        subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(private), "-in", str(document), "-out", str(signature)], check=True, capture_output=True)
        (output / "update.json.sig").write_text(base64.b64encode(signature.read_bytes()).decode() + "\n", encoding="ascii")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--notes", type=Path, required=True)
    args = parser.parse_args()
    private_key = os.environ.get("UPDATE_SIGNING_KEY", "")
    if not private_key:
        raise SystemExit("UPDATE_SIGNING_KEY secret is required; unsigned releases are not published")
    manifest = create(args.artifacts, args.tag, args.notes.read_text(encoding="utf-8"), private_key, args.output)
    print(f"Verified and signed {len(manifest['artifacts'])} platform packages for {args.tag}")
