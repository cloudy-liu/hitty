"""Provenance checks must catch a partial sync, changed bytes, and engine drift."""

import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("sync_agent_detection", Path(__file__).with_name("sync-agent-detection.py"))
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)


class ProvenanceChecks(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        shutil.copytree(sync.ROOT / "assets/agent-detection", self.root / "assets/agent-detection")
        (self.root / "src/terminal").mkdir(parents=True)
        shutil.copyfile(sync.ROOT / "src/terminal/screen_status.rs", self.root / "src/terminal/screen_status.rs")

    def test_every_pinned_file_passes_offline(self):
        sync.check(self.root)

    def test_partial_sync_fails_even_when_other_agents_are_current(self):
        (self.root / "assets/agent-detection/antigravity.toml").unlink()
        with self.assertRaisesRegex(ValueError, "every bundled manifest"):
            sync.check(self.root)

    def test_same_version_with_changed_rule_is_not_upstream(self):
        path = self.root / "assets/agent-detection/pi.toml"
        path.write_bytes(path.read_bytes().replace(b"Working", b"Idle"))
        with self.assertRaisesRegex(ValueError, "pi.toml: differs"):
            sync.check(self.root)

    def test_engine_downgrade_cannot_silently_load_newer_rules(self):
        path = self.root / "src/terminal/screen_status.rs"
        path.write_text(path.read_text(encoding="utf-8").replace("const MANIFEST_ENGINE_VERSION: u32 = 3;", "const MANIFEST_ENGINE_VERSION: u32 = 1;"), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "requires a newer detection engine"):
            sync.check(self.root)


if __name__ == "__main__":
    unittest.main()
