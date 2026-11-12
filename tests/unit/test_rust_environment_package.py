"""Offline exporter regressions; fake DLL bytes are never executed."""
import contextlib
import hashlib
import importlib.util
import io
import json
import pathlib
import sys
import tempfile
import unittest
import zipfile
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('rust_environment_package', ROOT/'tools/dev/package-rust-environment.py')
EXPORTER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPORTER)


class RustEnvironmentPackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.agent = self.root/'agent'
        self.output = self.root/'export'
        (self.agent/'extensions').mkdir(parents=True)
        (self.agent/'skills').mkdir()
        for name in EXPORTER.EXTENSIONS:
            (self.agent/'extensions'/f'rpi_{name}.dll').write_bytes(b'non-executable fixture')
        self.skill('rust-router', 'rust-router')
        self.skill('dioxus', 'dioxus-knowledge-patch')
        # Dioxus is deliberately absent from the generated global registry.
        self.registry([{'name':'rust-router','category':'rust','path':'skill-library/rust-router/SKILL.md'}])
        (self.agent/'auth.json').write_text('SECRET_MUST_NOT_BE_EXPORTED',encoding='utf8')
        (self.agent/'settings.json').write_text('SECRET_MUST_NOT_BE_EXPORTED',encoding='utf8')

    def registry(self, entries):
        (self.agent/'skills/registry.json').write_text(json.dumps({'skills':entries}),encoding='utf8')

    def skill(self, folder, name):
        target = self.agent/'skill-library'/folder
        target.mkdir(parents=True,exist_ok=True)
        (target/'SKILL.md').write_text(f'---\nname: {name}\ndescription: Local test skill.\n---\n# Fixture\n',encoding='utf8')
        return target

    def export(self):
        def capture(*args):
            return 'rustc 1.99.1 (fixture)\nhost: x86_64-pc-windows-msvc' if args[0]=='rustc' else 'offline fixture'
        with patch.object(sys,'argv',['exporter','--agent-dir',str(self.agent),'--output',str(self.output)]), patch.object(EXPORTER,'capture',side_effect=capture), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            EXPORTER.main()

    def test_unregistered_dioxus_uses_frontmatter_name_and_keeps_references(self):
        dioxus = self.agent/'skill-library/dioxus'
        (dioxus/'references').mkdir()
        (dioxus/'references/signals.md').write_text('# Signals\n',encoding='utf8')
        self.export()
        package = self.output/'rust-development'
        self.assertTrue((package/'skills/dioxus-knowledge-patch/SKILL.md').is_file())
        self.assertTrue((package/'skills/dioxus-knowledge-patch/references/signals.md').is_file())
        self.assertFalse((package/'skills/dioxus').exists())
        self.assertIn('../dioxus-knowledge-patch/SKILL.md',(package/'skills/rust-development/SKILL.md').read_text(encoding='utf8'))
        for entry in json.loads((self.output/'checksums.json').read_text(encoding='utf8')):
            raw = (package/entry['path']).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),entry['sha256'])
            self.assertNotIn(b'SECRET_MUST_NOT_BE_EXPORTED',raw)
        with zipfile.ZipFile(self.output/'rust-development-1.0.0-windows-x64.zip') as archive:
            self.assertIsNone(archive.testzip())
            self.assertIn('selection.json',archive.namelist())
            self.assertFalse(any('auth.json' in name or 'settings.json' in name for name in archive.namelist()))
        self.assertEqual(json.loads((self.output/'selection.json').read_text(encoding='utf8'))['main_package']['scope'],'project')

    def test_existing_export_is_not_overwritten(self):
        self.output.mkdir()
        marker = self.output/'preserve.txt'
        marker.write_text('keep',encoding='utf8')
        with self.assertRaises(SystemExit): self.export()
        self.assertEqual(marker.read_text(encoding='utf8'),'keep')

    def test_registry_cannot_escape_agent(self):
        self.registry([{'name':'escape','category':'rust','path':'../outside/SKILL.md'}])
        with self.assertRaisesRegex(ValueError,'invalid Skill registry path'): self.export()

    def test_missing_dioxus_refuses_dangling_local_router(self):
        import shutil
        shutil.rmtree(self.agent/'skill-library/dioxus')
        with self.assertRaisesRegex(ValueError,'required Rust router/Dioxus Skill missing'): self.export()
        self.assertFalse((self.output/'rust-development-1.0.0-windows-x64.zip').exists())

    def test_invalid_frontmatter_name_is_not_used_as_a_directory(self):
        self.skill('dioxus','../escape')
        with self.assertRaisesRegex(ValueError,'invalid Skill name'): self.export()


if __name__=='__main__': unittest.main()
