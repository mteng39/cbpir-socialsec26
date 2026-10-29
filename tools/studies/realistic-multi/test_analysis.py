import copy
import json
from pathlib import Path
import tempfile
import unittest

from analysis import analyze, load_jsonl, write_outputs


ROOT = Path(__file__).resolve().parents[4]
K1 = ROOT / "docs/project/experiments/p0d/2026-09-11/bgl-k1-study"
OUT = ROOT / "docs/project/experiments/p0d/2026-09-12/kgt1-bgl"
P3 = ROOT / "docs/project/experiments/p0d/2026-09-12/p3-standalone-validation/p3_report.json"


class AnalysisTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = json.loads((K1 / "catalog.json").read_text())
        cls.cases = load_jsonl(K1 / "cases.jsonl")
        cls.native = load_jsonl(OUT / "native.jsonl")
        cls.ckks = json.loads((K1 / "ckks_sizes.json").read_text())
        cls.p3 = json.loads(P3.read_text())

    def test_retained_run_matches_independent_k4_oracle_and_exact_fetch(self):
        result = analyze(self.catalog, self.cases, self.native, self.ckks, self.p3)
        summary = result["summary"]
        self.assertEqual(summary["requests"], 643)
        self.assertEqual(summary["execution_errors"], 0)
        self.assertEqual(summary["fetch_payloads_inexact"], 0)
        self.assertEqual(summary["nonzero_unselected_window_requests"], 0)
        self.assertEqual(summary["oracle_candidate_size_max"], 10)
        self.assertEqual(summary["oracle_over_k_requests"], 2)
        self.assertEqual(summary["oracle_literal_multi_record_requests"], 147)
        self.assertEqual(sum(row["requests"] for row in result["candidate_distribution"]), 643)
        self.assertEqual(len(result["attempts"]), 643)
        self.assertEqual(len(result["timings"]), 643)

    def test_rejects_one_changed_payload(self):
        changed = copy.deepcopy(self.native)
        row = next(row for row in changed[1:] if row["run"]["outputs"])
        row["run"]["outputs"][0]["payload"][0] ^= 1
        with self.assertRaises(ValueError):
            analyze(self.catalog, self.cases, changed, self.ckks, self.p3)

    def test_writes_csv_and_json_outputs(self):
        result = analyze(self.catalog, self.cases, self.native, self.ckks, self.p3)
        with tempfile.TemporaryDirectory() as directory:
            paths = write_outputs(Path(directory), result)
            self.assertEqual(len(paths), 6)
            self.assertTrue(all(path.stat().st_size > 20 for path in paths))


if __name__ == "__main__":
    unittest.main()
