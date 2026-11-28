"""Collect exactly one package per platform and sign the bytes clients verify."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
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


GPUI_ARTIFACTS = {
    "x86_64-pc-windows-msvc": "rpi-work-windows-x64.zip",
    "aarch64-pc-windows-msvc": "rpi-work-windows-arm64.zip",
    "x86_64-apple-darwin": "rpi-work-macos-x64.zip",
    "aarch64-apple-darwin": "rpi-work-macos-arm64.zip",
    "x86_64-unknown-linux-gnu": "rpi-work-linux-x64.tar.gz",
    "aarch64-unknown-linux-gnu": "rpi-work-linux-arm64.tar.gz",
}


def create(root: Path, tag: str, notes: str, key_pem: str, output: Path, *, gpui: bool = False) -> dict:
    repo = Path(__file__).resolve().parents[2]
    app = "work-gpui" if gpui else "work-ui"
    version = tomllib.loads((repo / "apps" / app / "Cargo.toml").read_text())["package"]["version"]
    if tag != f"v{version}":
        raise ValueError(f"Release tag must match the {app} package version")
    if gpui and not re.fullmatch(r"(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}", version):
        raise ValueError("GPUI updates require a stable version")

    packages = GPUI_ARTIFACTS if gpui else {
        target: f"rpi-work-{version}-{target}.{extension}"
        for target, extension in TARGETS.items()
    }
    output.mkdir(parents=True, exist_ok=True)
    artifacts = {}
    for target, name in packages.items():
        matches = list(root.rglob(name))
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one {name}, found {len(matches)}")
        source = matches[0]
        size = source.stat().st_size
        if not 0 < size <= 256 * 1024 * 1024:
            raise ValueError(f"Invalid package size: {name}")
        with source.open("rb") as package:
            digest = hashlib.file_digest(package, "sha256").hexdigest()
        checksum = source.with_name(name + ".sha256").read_text()
        if gpui:
            checksum_matches = checksum.rstrip("\n") == f"{digest}  {name}"
        else:
            checksum_matches = checksum.split()[:1] == [digest]
        if not checksum_matches:
            raise ValueError(f"Checksum mismatch: {name}")
        artifacts[target] = {"name": name, "size": size, "sha256": digest}
        shutil.copyfile(source, output / name)
        (output / (name + ".sha256")).write_text(f"{digest}  {name}\n", encoding="ascii")
    manifest = {"schema": 1, "version": version, "notes": notes, "artifacts": artifacts}
    manifest_name = "gpui-update.json" if gpui else "update.json"
    document = output / manifest_name
    encoded = (json.dumps(manifest, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()
    if gpui and len(encoded) > 256 * 1024:
        raise ValueError("GPUI update manifest exceeds the client size limit")
    document.write_bytes(encoded)
    with tempfile.TemporaryDirectory(prefix="rpi-update-sign-") as tmp:
        private = Path(tmp) / "private.pem"
        private.write_text(key_pem, encoding="ascii")
        private.chmod(0o600)
        public = subprocess.run(["openssl", "pkey", "-in", str(private), "-pubout", "-outform", "DER"], check=True, capture_output=True).stdout
        expected = base64.b64decode((repo / "crates/update/work-update/update-public-key.txt").read_text().strip(), validate=True)
        if public[:12] != bytes.fromhex("302a300506032b6570032100") or public[12:] != expected:
            raise ValueError("Signing key does not match the public key embedded in the client")
        signature = Path(tmp) / "signature.bin"
        subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(private), "-in", str(document), "-out", str(signature)], check=True, capture_output=True)
        (output / f"{manifest_name}.sig").write_text(base64.b64encode(signature.read_bytes()).decode() + "\n", encoding="ascii")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--notes", type=Path, required=True)
    parser.add_argument("--gpui", action="store_true", help="sign the GPUI desktop packages")
    args = parser.parse_args()
    private_key = os.environ.get("UPDATE_SIGNING_KEY", "")
    if not private_key:
        raise SystemExit("UPDATE_SIGNING_KEY secret is required; unsigned releases are not published")
    manifest = create(
        args.artifacts, args.tag, args.notes.read_text(encoding="utf-8"), private_key, args.output,
        gpui=args.gpui,
    )
    print(f"Verified and signed {len(manifest['artifacts'])} platform packages for {args.tag}")
