#!/usr/bin/env python3
"""Contract tests for the public repository layout."""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class PublicLayoutTests(unittest.TestCase):
    def test_root_workspace_has_supported_entry_points(self) -> None:
        required = (
            "Cargo.toml",
            "Cargo.lock",
            "src/lib.rs",
            "src/bin/cbpir.rs",
            "src/bin/benchmark.rs",
            "src/bin/pattern_length.rs",
            "src/bin/scalability.rs",
            "src/bin/search_correctness.rs",
            "crates/bgv-cpir/Cargo.toml",
            "crates/ckks-search/Cargo.toml",
            "crates/multi-fetch/Cargo.toml",
            "openfhe/bootstrap_openfhe.sh",
        )
        missing = [path for path in required if not (ROOT / path).is_file()]
        self.assertEqual([], missing)

    def test_generated_state_is_not_tracked(self) -> None:
        tracked = subprocess.run(
            ["git", "ls-files"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.splitlines()
        generated = [
            path
            for path in tracked
            if "__pycache__" in Path(path).parts
            or ".build-cache" in Path(path).parts
            or "target" in Path(path).parts
            or path.endswith((".pyc", ".pyo"))
        ]
        self.assertEqual([], generated)

    def test_active_interface_does_not_expose_prototype_paths(self) -> None:
        active_files = (
            ROOT / "README.md",
            ROOT / "REPRODUCE.md",
            ROOT / "MANIFEST.md",
            ROOT / "Cargo.toml",
        )
        forbidden = (
            "code_branch/prototypes",
            "P0-A_bgv_cpir",
            "P0-C_ckks_dft_search",
            "P0-D_ckks_single_record_composition",
            "P3_multi_record_cpir",
        )
        violations: list[str] = []
        for path in active_files:
            if not path.is_file():
                violations.append(f"missing:{path.relative_to(ROOT)}")
                continue
            text = path.read_text(encoding="utf-8")
            for token in forbidden:
                if token in text:
                    violations.append(f"{path.relative_to(ROOT)}:{token}")
        self.assertEqual([], violations)

    def test_active_build_helpers_do_not_require_private_workspace_paths(self) -> None:
        active_files = (
            ROOT / "scripts/common/workspace_build_lock.py",
            ROOT / "scripts/common/workspace_build_lock.sh",
            ROOT / "scripts/benchmark/run.py",
            ROOT / "scripts/search-correctness/run.py",
            ROOT / "scripts/pattern-length/run.py",
            ROOT / "scripts/scalability/run.py",
        )
        forbidden = (
            "/state" + "/dev",
            "/workspace" + "/codex_projects",
            "/home" + "/rae",
        )
        violations = [
            f"{path.relative_to(ROOT)}:{token}"
            for path in active_files
            for token in forbidden
            if token in path.read_text(encoding="utf-8")
        ]
        self.assertEqual([], violations)


if __name__ == "__main__":
    unittest.main(verbosity=2)
