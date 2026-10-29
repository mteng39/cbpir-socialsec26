#!/usr/bin/env python3

import importlib.util
import csv
import json
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "run.py"


def load_module():
    spec = importlib.util.spec_from_file_location("pattern_length_v1", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PatternLengthContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_module()
        cls.root = HERE.parents[1]

    def test_matrix_has_24_configurations_and_480_measured_runs(self):
        matrix = self.module.expected_matrix()
        self.assertEqual(24, len(matrix))
        self.assertEqual({128, 256, 512}, {row["n"] for row in matrix})
        self.assertEqual({1, 4, 8, 16}, {row["pattern_length"] for row in matrix})
        self.assertEqual({"fps", "rfps"}, {row["mode"] for row in matrix})
        self.assertTrue(all(row["public_k"] == 16 for row in matrix))
        self.assertEqual(480, len(matrix) * self.module.MEASURED_RUNS)

    def test_payload_reuses_plaintext_selected_natural_patterns(self):
        source = json.loads(
            (self.root / "data/workloads/formal_8192_input.json").read_text()
        )
        payload = self.module.build_native_input(source)
        self.assertEqual("cbpir-pattern-length-v1-input", payload["schema"])
        self.assertEqual([1, 4, 8, 16], [len(row["pattern"]) for row in payload["query_fixtures"]])
        self.assertEqual(["fps", "rfps"], payload["modes"])
        self.assertEqual(16, payload["public_k"])
        self.assertEqual(3, payload["warmups"])
        self.assertEqual(20, payload["measured_runs"])
        for fixture in payload["query_fixtures"]:
            self.assertGreater(fixture["expected_literal_alignment_count"], 0)
            self.assertGreater(fixture["expected_literal_record_count"], 0)

    def test_summary_keeps_both_online_conventions(self):
        durations = [
            {
                "search_query_ns": 1,
                "search_response_ns": 0,
                "locate_ns": 10,
                "fetch_query_ns": 4,
                "fetch_response_ns": 4,
                "recover_ns": 2,
                "online_sum_ns": 21,
                "online_outer_ns": 22,
            },
            {
                "search_query_ns": 5,
                "search_response_ns": 5,
                "locate_ns": 1,
                "fetch_query_ns": 4,
                "fetch_response_ns": 4,
                "recover_ns": 2,
                "online_sum_ns": 21,
                "online_outer_ns": 23,
            },
            {
                "search_query_ns": 5,
                "search_response_ns": 5,
                "locate_ns": 10,
                "fetch_query_ns": 1,
                "fetch_response_ns": 0,
                "recover_ns": 0,
                "online_sum_ns": 21,
                "online_outer_ns": 24,
            },
        ]
        summary = self.module.summarize_durations(durations)
        self.assertEqual(21, summary["median_per_run_online_sum_ns"])
        self.assertEqual(30, summary["sum_of_stage_medians_ns"])
        self.assertEqual(23, summary["median_online_outer_ns"])

    def test_native_temp_directory_avoids_system_tmpfs(self):
        environment = self.module.benchmark_env()
        temporary = Path(environment["TMPDIR"])
        self.assertTrue(temporary.is_dir())
        self.assertNotEqual(Path("/tmp"), temporary)
        self.assertTrue(str(temporary).startswith(str(self.module.TARGET)))

    def test_retained_evidence_has_complete_validated_matrix(self):
        evidence = self.root / "evaluation/pattern-length-v1"
        with (evidence / "raw.csv").open(newline="", encoding="utf-8") as handle:
            raw = list(csv.DictReader(handle))
        with (evidence / "summary.csv").open(newline="", encoding="utf-8") as handle:
            summary = list(csv.DictReader(handle))
        self.assertEqual(480, len(raw))
        self.assertEqual(24, len(summary))
        self.assertTrue(all(row["success"] == "True" for row in raw))
        self.assertTrue(all(row["fetch_exact_reconstruction"] == "True" for row in raw))
        self.assertTrue(all(row["query_ciphertext_count"] == "1" for row in raw))
        self.assertTrue(all(row["response_ciphertext_count"] == "1" for row in raw))
        for row in raw:
            expected = sum(
                int(row[field])
                for field in (
                    "search_query_ns",
                    "search_response_ns",
                    "locate_ns",
                    "fetch_query_ns",
                    "fetch_response_ns",
                    "recover_ns",
                )
            )
            self.assertEqual(expected, int(row["online_sum_ns"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
