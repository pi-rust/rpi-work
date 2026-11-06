"""Publish only after every signed asset has been uploaded to a draft release."""
import json
import os
from pathlib import Path
import subprocess
import hashlib
from urllib.parse import urlparse

tag = os.environ["GITHUB_REF_NAME"]
repo = os.environ["GITHUB_REPOSITORY"]
directory = Path("target/release-upload")
manifest = json.loads((directory / "update.json").read_text(encoding="utf-8"))
if tag != "v" + manifest["version"]:
    raise SystemExit("Tag and signed manifest differ")
assets = [str(directory / artifact["name"]) for artifact in manifest["artifacts"].values()]
assets += [asset + ".sha256" for asset in assets]
assets += [str(directory / "update.json"), str(directory / "update.json.sig")]
component = directory / "rpi-component-http.json"
bundle = json.loads(component.read_text(encoding="utf-8"))
canonical=json.dumps([bundle["manifest"],bundle["files"]],ensure_ascii=False,separators=(",",":")).encode()
if hashlib.sha256(canonical).hexdigest()!=bundle["sha256"]:
    raise SystemExit("Native component bundle digest validation failed")
native = bundle["manifest"]["runtime"]["artifacts"]
assets += [str(component)]
for target, artifact in native.items():
    name = Path(urlparse(artifact["url"]).path).name
    path = directory / name
    if not name.startswith("rpi-http-native-") or path.stat().st_size != artifact["bytes"] or hashlib.sha256(path.read_bytes()).hexdigest() != artifact["sha256"]:
        raise SystemExit("Native component artifact validation failed")
    assets.append(str(path))
result = subprocess.run(["gh", "release", "view", tag, "--repo", repo, "--json", "isDraft"], capture_output=True, text=True)
if result.returncode == 0:
    if not json.loads(result.stdout)["isDraft"]:
        raise SystemExit("This release is already public; published update assets must not be overwritten")
else:
    subprocess.run(["gh", "release", "create", tag, "--repo", repo, "--verify-tag", "--draft", "--title", f"RPI Work {tag}", "--notes-file", "docs/release-notes.md"], check=True)
subprocess.run(["gh", "release", "upload", tag, "--repo", repo, "--clobber", *assets], check=True)
subprocess.run(["gh", "release", "edit", tag, "--repo", repo, "--draft=false", "--latest"], check=True)
