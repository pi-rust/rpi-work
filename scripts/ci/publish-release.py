"""Publish and synchronize every signed Work asset for the release tag."""
import json
import os
from pathlib import Path
import subprocess

tag = os.environ["GITHUB_REF_NAME"]
repo = os.environ["GITHUB_REPOSITORY"]
directory = Path("target/release-upload")
manifest = json.loads((directory / "update.json").read_text(encoding="utf-8"))
if tag != "v" + manifest["version"]:
    raise SystemExit("Tag and signed manifest differ")
assets = [str(directory / artifact["name"]) for artifact in manifest["artifacts"].values()]
assets += [asset + ".sha256" for asset in assets]
assets += [str(directory / "update.json"), str(directory / "update.json.sig")]
result = subprocess.run(
    ["gh", "release", "view", tag, "--repo", repo, "--json", "assets"],
    capture_output=True,
    text=True,
)
if result.returncode != 0:
    subprocess.run(
        [
            "gh",
            "release",
            "create",
            tag,
            "--repo",
            repo,
            "--verify-tag",
            "--draft",
            "--title",
            f"RPI Work {tag}",
            "--notes-file",
            "docs/release-notes.md",
        ],
        check=True,
    )
else:
    # A rebuilt tag is intentionally authoritative: remove assets from an older
    # build before clobbering the names supplied by this run. This also permits
    # correcting a public release after a failed or incomplete publication.
    existing = json.loads(result.stdout).get("assets", [])
    expected_names = {Path(asset).name for asset in assets}
    for asset in existing:
        name = asset.get("name")
        if name and name not in expected_names:
            subprocess.run(
                ["gh", "release", "delete-asset", tag, name, "--repo", repo, "--yes"],
                check=True,
            )

subprocess.run(["gh", "release", "upload", tag, "--repo", repo, "--clobber", *assets], check=True)
subprocess.run(
    [
        "gh",
        "release",
        "edit",
        tag,
        "--repo",
        repo,
        "--title",
        f"RPI Work {tag}",
        "--notes-file",
        "docs/release-notes.md",
        "--draft=false",
        "--latest",
    ],
    check=True,
)
