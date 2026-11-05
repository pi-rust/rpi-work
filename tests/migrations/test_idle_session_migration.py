"""Isolated migration checks; never access real user files or start processes."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("migration", Path(__file__).parents[2] / "tools/dev/migrate-idle-sessions.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class MigrationTests(unittest.TestCase):
    def test_merge_preserves_unknown_fields_and_metadata_without_mutating_source(self):
        old = {"sessions": [{"sessionId": "existing", "unknown": [1]}], "extra": {"version": 9}, "preferences": {"existing": {"tags": ["keep"]}}}
        source = {"sessions": [{"sessionId": "idle", "title": "旧会话", "createdAt": 4, "updatedAt": 8, "parentSessionId": "parent"}, {"sessionId": "active"}], "preferences": {"idle": {"archived": True, "pinned": True, "tags": ["tag"]}}}
        before = copy.deepcopy((old, source))
        result, ids = m.merge(old, [source], {"active"})
        self.assertEqual(ids, ["idle"])
        self.assertEqual(result["sessions"][:1], old["sessions"])
        self.assertEqual(result["extra"], old["extra"])
        self.assertEqual(result["preferences"]["idle"], source["preferences"]["idle"])
        self.assertEqual((old, source), before)

    def test_duplicate_ids_and_existing_target_collisions_fail_closed(self):
        with self.assertRaises(ValueError):
            m.registry({"sessions": [{"sessionId": "id"}, {"sessionId": "id"}]})
        with self.assertRaises(ValueError):
            m.merge({"sessions": [{"sessionId": "id"}]}, [{"sessions": [{"sessionId": "id"}]}], set())

    def test_history_rejects_truncation_corruption_or_wrong_identity(self):
        good = json.dumps({"kind": "header", "id": "id", "cwd": "project"}).encode() + b"\n"
        self.assertEqual(m.validate_history(good, "id")[1], 1)
        for raw, sid in [(good[:-1], "id"), (good + b"{bad}\n", "id"), (good, "different")]:
            with self.assertRaises(ValueError):
                m.validate_history(raw, sid)

    def test_apply_uses_separate_verified_history_store_and_never_changes_source(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            project = root / "project"
            project.mkdir()
            source, target = root / "source", root / "target"
            pid = m.project_id(project)
            index = source / "rpc/projects" / pid / "sessions.json"
            index.parent.mkdir(parents=True)
            (source / "rpc/projects.json").write_text(json.dumps([{"path": str(project), "name": "project"}]), encoding="utf-8")
            src = {"sessions": [{"sessionId": "idle", "projectId": pid, "title": "保留", "createdAt": 1}, {"sessionId": "active", "projectId": pid}], "preferences": {"idle": {"archived": True}}}
            index.write_text(json.dumps(src), encoding="utf-8")
            history = project / ".rpi/sessions" / m.cwd_folder(str(project)) / "date_idle.jsonl"
            history.parent.mkdir(parents=True)
            raw = (json.dumps({"kind": "header", "id": "idle", "cwd": str(project)}) + "\n" + json.dumps({"kind": "record", "type": "message", "text": "test"}) + "\n").encode()
            history.write_bytes(raw)
            target.mkdir()
            old = {"sessions": [{"sessionId": "preexisting-unresolved", "projectId": "unavailable"}], "extra": 3}
            target_index = target / "sessions.json"
            target_index.write_text(json.dumps(old), encoding="utf-8")
            source_before = index.read_bytes()
            target_before = target_index.read_bytes()
            args = argparse.Namespace(source_data=source, http_data=target, default_project=project, exclude_session=["active"], apply=False, port=0, rpi_bin="rpi")
            m.run(args)
            self.assertEqual(target_index.read_bytes(), target_before)
            args.apply = True
            result = m.run(args)
            self.assertEqual(index.read_bytes(), source_before)
            self.assertEqual(history.read_bytes(), raw)
            self.assertEqual(m.read_json(target_index)["sessions"][0], old["sessions"][0])
            self.assertEqual(len(m.read_json(target_index)["sessions"]), 2)
            copied = Path(result["store"]) / result["histories"]["idle"]["relative"]
            self.assertEqual(copied.read_bytes(), raw)
            self.assertNotEqual(copied, history)
            backup = Path(result["config"]).parent / "migrations"
            saved = list(backup.glob("*/histories/*/*.jsonl"))
            self.assertEqual(len(saved), 1)
            self.assertEqual(saved[0].read_bytes(), raw)
            self.assertEqual(m.read_json(Path(result["config"]))["bind"], "127.0.0.1")
            with self.assertRaises(ValueError):
                m.run(args)  # Existing IDs cannot be duplicated by a rerun.


if __name__ == "__main__":
    unittest.main()
