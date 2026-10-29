import json
import copy
from pathlib import Path
import tempfile
import unittest

from analysis import analyze, write_outputs


ROOT = Path(__file__).resolve().parents[4]
REPORT = ROOT / "docs/project/experiments/p0d/2026-09-12/kgt1-synthetic/p0d_kgt1_report.json"
CKKS = ROOT / "docs/project/experiments/p0d/2026-09-10/validation/ckks_sizes.json"
P3 = ROOT / "docs/project/experiments/p0d/2026-09-12/p3-standalone-validation/p3_report.json"


class AnalysisTests(unittest.TestCase):
    def test_retained_report_has_separate_counts_and_composed_sizes(self):
        result = analyze(
            json.loads(REPORT.read_text()),
            json.loads(CKKS.read_text()),
            json.loads(P3.read_text()),
        )
        self.assertEqual(result["summary"]["measured_requests"], 42)
        self.assertEqual(result["summary"]["execution_errors"], 0)
        self.assertEqual(result["summary"]["fetch_payloads_inexact"], 0)
        self.assertEqual(result["summary"]["ckks_decision_disagreements"], 0)
        self.assertEqual(result["summary"]["false_selected_records"], 9)
        self.assertEqual(result["messages"]["fps"]["client_to_server_bytes"], 1_969_043)
        self.assertEqual(result["messages"]["rfps"]["client_to_server_bytes"], 920_223)
        self.assertEqual(result["messages"]["p3"]["public_k"], 4)
        self.assertEqual(len(result["cases"]), 14)
        self.assertEqual(len(result["timings"]), 42)

    def test_writes_machine_readable_outputs(self):
        result = analyze(
            json.loads(REPORT.read_text()),
            json.loads(CKKS.read_text()),
            json.loads(P3.read_text()),
        )
        with tempfile.TemporaryDirectory() as directory:
            paths = write_outputs(Path(directory), result)
            self.assertEqual({path.name for path in paths}, {
                "summary.json", "messages.json", "cases.csv", "timings.csv"
            })
            self.assertTrue(all(path.stat().st_size > 20 for path in paths))

    def test_rejects_failed_gate_and_changed_payload(self):
        report = json.loads(REPORT.read_text())
        ckks = json.loads(CKKS.read_text())
        p3 = json.loads(P3.read_text())

        failed = copy.deepcopy(report)
        failed["bounded_fixture_gate_passed"] = False
        with self.assertRaises(AssertionError):
            analyze(failed, ckks, p3)

        changed = copy.deepcopy(report)
        case = next(case for case in changed["cases"] if case["repetitions"][0]["outputs"])
        case["repetitions"][0]["outputs"][0]["payload"][0] ^= 1
        with self.assertRaises(AssertionError):
            analyze(changed, ckks, p3)


if __name__ == "__main__":
    unittest.main()
