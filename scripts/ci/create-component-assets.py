"""Assemble an installable UI/native component from this run's verified artifacts.

Does not publish, execute native code or alter the signed Work update contract.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
from urllib.parse import quote

TARGETS = {
    "x86_64-pc-windows-msvc": ".dll", "aarch64-pc-windows-msvc": ".dll",
    "x86_64-apple-darwin": ".dylib", "aarch64-apple-darwin": ".dylib",
    "x86_64-unknown-linux-gnu": ".so", "aarch64-unknown-linux-gnu": ".so",
}


def build(root: Path, output: Path, repository: str, tag: str) -> Path:
    if len(repository.split("/")) != 2 or any(not part or not all(c.isalnum() or c in "-_." for c in part) for part in repository.split("/")):
        raise ValueError("Invalid release repository")
    templates = list(root.rglob("rpi-component-ui-*.json"))
    if len(templates) != len(TARGETS):
        raise ValueError("Missing or duplicate native component UI templates")
    template = json.loads(templates[0].read_text(encoding="utf-8-sig"))
    if any(json.loads(p.read_text(encoding="utf-8-sig")) != template for p in templates[1:]):
        raise ValueError("Native builds disagree on the component UI")
    manifest, files = template["manifest"], template["files"]
    compact = lambda value: json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
    if hashlib.sha256(compact([manifest, files])).hexdigest() != template["sha256"]:
        raise ValueError("Component UI digest mismatch")
    output.mkdir(parents=True, exist_ok=True)
    artifacts = {}
    for target, suffix in sorted(TARGETS.items()):
        name = f"rpi-http-native-{target}{suffix}"
        matches = list(root.rglob(name))
        if len(matches) != 1 or not 0 < matches[0].stat().st_size <= 256 * 1024 * 1024:
            raise ValueError(f"Missing, duplicate or oversized native artifact: {name}")
        source = matches[0]
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        artifacts[target] = {"url": f"https://github.com/{repository}/releases/download/{quote(tag, safe='')}/{name}", "sha256": digest, "bytes": source.stat().st_size}
        shutil.copyfile(source, output / name)
    manifest["runtime"] = {"entryFlag": "http", "artifacts": artifacts}
    template["sha256"] = hashlib.sha256(compact([manifest, files])).hexdigest()
    path = output / "rpi-component-http.json"
    path.write_bytes(compact(template) + b"\n")
    return path


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args()
    print(build(args.artifacts, args.output, args.repository, args.tag))
