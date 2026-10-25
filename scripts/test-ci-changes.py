#!/usr/bin/env python3
"""Keep CI change filtering conservative as the workspace grows."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("ci_changes", Path(__file__).with_name("ci-changes.py"))
changes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(changes)


class BuildInputs(unittest.TestCase):
    def test_editorial_and_release_configuration_keep_quality_checks_only(self):
        self.assertFalse(changes.requires_build([
            "README.md", "docs/skia.md", "AGENTS.md", "release-plz.toml", ".github/dependabot.yml",
        ]))

    def test_code_dependencies_build_tools_and_unknown_files_keep_full_coverage(self):
        for path in ["crates/new/src/lib.rs", "Cargo.lock", "Cargo.toml", ".cargo/config.toml",
                     ".github/workflows/ci.yml", ".github/actions/setup-native/action.yml",
                     "scripts/check-hotreload.py", "crates/plico-lvgl/config/lv_conf.h", "new.unknown"]:
            with self.subTest(path=path):
                self.assertTrue(changes.requires_build(["README.md", path]))

    def test_removed_source_still_requires_a_build_when_renamed_to_documentation(self):
        # git diff --no-renames emits both names, including the removed Rust source.
        self.assertTrue(changes.requires_build(["crates/old.rs", "docs/old.md"]))


if __name__ == "__main__":
    unittest.main()
