"""Copy idle native RPC sessions into an offline HTTP node without touching source data.

Default is a read-only preview. --apply makes a timestamped backup, a separate
HTTP transcript store, startup config, and an atomic target registry replacement.
Does not start, stop, or signal processes; never reads credentials.
"""
import argparse
import contextlib
import copy
import datetime
import hashlib
import json
import os
from pathlib import Path
import socket
import tempfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def registry(value):
    if not isinstance(value, dict) or not isinstance(value.get("sessions"), list):
        raise ValueError("invalid registry")
    ids = [row["sessionId"] for row in value["sessions"]]
    if any(not isinstance(sid, str) or not sid for sid in ids) or len(ids) != len(set(ids)):
        raise ValueError("empty/duplicate session ID")
    if not isinstance(value.get("preferences", {}), dict):
        raise ValueError("invalid preferences")
    return value


def merge(target, sources, excluded):
    result = copy.deepcopy(registry(target))
    known = {row["sessionId"] for row in result["sessions"]}
    imported = []
    seen = set()
    for source in sources:
        registry(source)
        for row in source["sessions"]:
            sid = row["sessionId"]
            if sid in seen:
                raise ValueError("duplicate source ID: " + sid)
            seen.add(sid)
            if sid in excluded:
                continue
            if sid in known:
                raise ValueError("target already contains source ID: " + sid)
            result["sessions"].append(copy.deepcopy(row))
            if sid in source.get("preferences", {}):
                result.setdefault("preferences", {})[sid] = copy.deepcopy(source["preferences"][sid])
            known.add(sid)
            imported.append(sid)
    return result, imported


def project_id(path):
    canonical = str(Path(path).resolve()).removeprefix("\\\\?\\")
    key = canonical.lower() if os.name == "nt" else canonical
    return hashlib.sha256(key.encode()).hexdigest()[:16]


def cwd_folder(cwd):
    stripped = cwd[1:] if cwd.startswith(("/", "\\")) else cwd
    return "--" + stripped.replace("/", "-").replace("\\", "-").replace(":", "-") + "--"


def validate_history(raw, sid):
    if not raw.endswith(b"\n"):
        raise ValueError("incomplete JSONL: " + sid)
    lines = raw.splitlines()
    values = [json.loads(line) for line in lines]
    header = values[0]
    if header.get("kind") != "header" or header.get("id") != sid or not header.get("cwd"):
        raise ValueError("history identity mismatch: " + sid)
    return header, len(lines)


@contextlib.contextmanager
def history_lock(path):
    # rpi uses an exclusive fs2 lock on <jsonl>.lock. A one-byte lock overlaps
    # that whole-file lock on Windows. Never attempt to open/stop the host.
    lock = open(str(path) + ".lock", "a+b")
    acquired = False
    try:
        lock.seek(0)
        if os.name == "nt":
            import msvcrt
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        acquired = True
        yield
    finally:
        if acquired:
            lock.seek(0)
            if os.name == "nt":
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
        lock.close()


def offline(port):
    with socket.socket() as sock:
        sock.settimeout(0.5)
        if sock.connect_ex(("127.0.0.1", port)) == 0:
            raise RuntimeError("HTTP node is online; refusing registry writes")


def atomic_json(path, value):
    path = Path(path)
    fd, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as out:
            json.dump(value, out, ensure_ascii=False, indent=2)
            out.write("\n")
            out.flush()
            os.fsync(out.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def run(args):
    source = Path(args.source_data).resolve()
    target = Path(args.http_data).resolve()
    target_path = target / "sessions.json"
    original = target_path.read_bytes()
    old = registry(json.loads(original))
    aliases_path = source / "rpc/projects.json"
    aliases = read_json(aliases_path)
    projects = {project_id(row["path"]): row for row in aliases}
    files = sorted((source / "rpc/projects").glob("*/sessions.json"))
    source_bytes = {path: path.read_bytes() for path in files}
    sources = [registry(json.loads(raw)) for raw in source_bytes.values()]
    excluded = set(args.exclude_session)
    combined, candidates = merge(old, sources, excluded)
    lookup = {row["sessionId"]: row for row in combined["sessions"]}
    histories, skipped = {}, []
    missing_existing = []
    with contextlib.ExitStack() as locks:
        for sid, row in lookup.items():
            if sid in excluded:
                raise ValueError("excluded session is already present in target: " + sid)
            project = projects.get(row.get("projectId"))
            matches = [] if project is None else list((Path(project["path"]) / ".rpi/sessions").rglob("*_" + sid + ".jsonl"))
            if not matches and sid not in candidates:
                missing_existing.append(sid)
                continue
            if len(matches) != 1:
                if sid in candidates:
                    skipped.append({"id": sid, "reason": "missing or ambiguous history"})
                    continue
                raise ValueError("ambiguous existing target history: " + sid)
            path = matches[0]
            try:
                locks.enter_context(history_lock(path))
            except OSError:
                if sid not in candidates:
                    raise RuntimeError("existing HTTP history still has a live writer: " + sid)
                skipped.append({"id": sid, "reason": "session is locked by a live writer"})
                continue
            raw = path.read_bytes()
            header, lines = validate_history(raw, sid)
            if project_id(header["cwd"]) != row.get("projectId"):
                raise ValueError("project/history mismatch: " + sid)
            histories[sid] = {"source": str(path), "sha256": digest(raw), "bytes": len(raw), "lines": lines,
                              "relative": str(Path(cwd_folder(header["cwd"])) / path.name)}
        excluded.update(item["id"] for item in skipped)
        combined, imported = merge(old, sources, excluded)
        summary = {"before": len(old["sessions"]), "imported": imported, "after": len(combined["sessions"]),
                   "excluded": sorted(excluded), "skipped": skipped, "preexistingUnresolvedHistories": missing_existing}
        print(json.dumps(summary, ensure_ascii=True))
        if not args.apply:
            return summary
        offline(args.port)
        if not imported:
            raise RuntimeError("nothing to import")
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        backup = target / "migrations" / stamp
        backup.mkdir(parents=True, exist_ok=False)
        store = target / ("session-store-" + stamp)
        store.mkdir(exist_ok=False)
        config_path = target / ("node-migrated-" + stamp + ".json")
        manifest = {"state": "backing-up", **summary, "sourceData": str(source), "targetRegistry": str(target_path),
                    "targetBeforeSha256": digest(original), "store": str(store), "config": str(config_path), "histories": histories}
        journal = backup / "manifest.json"
        atomic_json(journal, manifest)
        (backup / "target-sessions.json").write_bytes(original)
        (backup / "source-projects.json").write_bytes(aliases_path.read_bytes())
        if (target / "projects.json").exists():
            (backup / "target-projects.json").write_bytes((target / "projects.json").read_bytes())
        for path, raw in source_bytes.items():
            (backup / ("source-" + path.parent.name + ".json")).write_bytes(raw)
        for sid, item in histories.items():
            src = Path(item["source"])
            raw = src.read_bytes()
            if digest(raw) != item["sha256"]:
                raise RuntimeError("history changed while staging: " + sid)
            saved = backup / "histories" / item["relative"]
            copied = store / item["relative"]
            saved.parent.mkdir(parents=True, exist_ok=True)
            copied.parent.mkdir(parents=True, exist_ok=True)
            saved.write_bytes(raw)
            copied.write_bytes(raw)
            if digest(copied.read_bytes()) != item["sha256"] or digest(saved.read_bytes()) != item["sha256"]:
                raise RuntimeError("copy verification failed: " + sid)
        # Preserve existing target timing data; add only non-conflicting source
        # timing files for the imported IDs, with byte-identical backups.
        timing_changes = []
        for sid in imported:
            sources_for_timing = list((source / "rpc/projects").glob("*/sessions.tool-timings/" + sid + ".json"))
            dest = target / "sessions.tool-timings" / (sid + ".json")
            for timing in sources_for_timing:
                raw = timing.read_bytes()
                saved = backup / "tool-timings" / (sid + ".json")
                saved.parent.mkdir(parents=True, exist_ok=True)
                saved.write_bytes(raw)
                if dest.exists() and dest.read_bytes() != raw:
                    raise RuntimeError("timing collision: " + sid)
                timing_changes.append((dest, raw))
        default = str(Path(args.default_project).resolve()).removeprefix("\\\\?\\")
        config = {"project": default, "projects": [str(Path(p["path"]).resolve()).removeprefix("\\\\?\\") for p in aliases if project_id(p["path"]) != project_id(default)],
                  "registry": str(target_path), "sessionDir": str(store), "rpiBin": args.rpi_bin, "bind": "127.0.0.1", "port": args.port}
        atomic_json(config_path, config)
        atomic_json(backup / "merged-sessions.json", combined)
        manifest["state"] = "prepared"
        atomic_json(journal, manifest)
        offline(args.port)
        if target_path.read_bytes() != original:
            raise RuntimeError("target registry changed; no registry commit")
        # Only compare selected rows; the excluded current session may keep
        # updating its own entry in the same source registry throughout.
        current_sources = [read_json(path) for path in files]
        selected_now, now_ids = merge(old, current_sources, excluded)
        if selected_now != combined or now_ids != imported:
            raise RuntimeError("source candidates changed; no registry commit")
        for dest, raw in timing_changes:
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(raw)
        atomic_json(target_path, combined)
        manifest["state"] = "committed-awaiting-http-verification"
        manifest["targetAfterSha256"] = digest(target_path.read_bytes())
        atomic_json(journal, manifest)
        print(json.dumps({"backup": str(backup), "config": str(config_path), "manifest": str(journal)}))
        return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-data", required=True)
    parser.add_argument("--http-data", required=True)
    parser.add_argument("--default-project", required=True)
    parser.add_argument("--rpi-bin", default="rpi")
    parser.add_argument("--port", type=int, default=13030)
    parser.add_argument("--exclude-session", action="append", default=[])
    parser.add_argument("--apply", action="store_true")
    run(parser.parse_args())
