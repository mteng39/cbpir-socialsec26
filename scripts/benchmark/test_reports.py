import csv
import copy
import importlib.util
import math
import json
from pathlib import Path
import tempfile
import unittest


HERE = Path(__file__).resolve().parent


def load(name):
    filename = "run.py" if name == "run_benchmark" else f"{name}.py"
    path = HERE / filename
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FormalWorkloadTests(unittest.TestCase):
    def test_frozen_workloads_and_timing_fixtures_are_exact(self):
        workload = load("formal_workload")
        studies = workload.build_inputs(warmups=3, measured_runs=20)
        expected = {
            8192: (128, 64, 3201, 4096, b"f", 16),
            16384: (256, 64, 6544, 8192, b",", 16),
            32768: (512, 64, 13358, 16384, b":", 17),
        }
        for ring, (n, ell, text_len, transform_len, pattern, candidates) in expected.items():
            study = studies[ring]
            self.assertEqual(study["workload"]["n"], n)
            self.assertEqual(study["workload"]["record_slots"], ell)
            self.assertEqual(study["workload"]["text_bytes"], text_len)
            self.assertEqual(study["workload"]["transform_len"], transform_len)
            self.assertEqual(bytes(study["timing_fixture"]["pattern"]), pattern)
            self.assertEqual(study["timing_fixture"]["expected_candidate_count"], candidates)
            self.assertEqual(study["modes"], ["fps", "rfps"])
            self.assertEqual(study["public_ks"], [1, 4, 8, 16])


class SummaryTests(unittest.TestCase):
    def test_summary_uses_sample_stddev_and_nearest_rank_p95(self):
        reports = load("run_benchmark")
        result = reports.summary([1_000_000, 2_000_000, 3_000_000, 4_000_000])
        self.assertEqual(result["samples"], 4)
        self.assertEqual(result["mean_ms"], 2.5)
        self.assertEqual(result["median_ms"], 2.5)
        self.assertTrue(math.isclose(result["stddev_ms"], 1.2909944487358056))
        self.assertEqual(result["min_ms"], 1.0)
        self.assertEqual(result["max_ms"], 4.0)
        self.assertEqual(result["p95_ms"], 4.0)

    def test_required_csv_names_are_fixed(self):
        reports = load("run_benchmark")
        self.assertEqual(
            reports.REQUIRED_OUTPUTS,
            (
                "00_environment.txt",
                "01_benchmark_report.md",
                "02_fixture_manifest.csv",
                "03_setup_raw.csv",
                "04_setup_summary.csv",
                "05_procedure_raw.csv",
                "06_procedure_summary.csv",
                "07_online_summary.csv",
                "08_communication.csv",
                "09_correctness.csv",
                "10_verification.txt",
                "11_fetch_k_scaling_raw.csv",
                "12_fetch_k_scaling_summary.csv",
            ),
        )

    def test_rfps_probe_gate_rejects_missing_or_changed_classification(self):
        reports = load("run_benchmark")
        outputs = {}
        for ring, expected in reports.EXPECTED_RFPS_PROBES.items():
            outputs[ring] = {
                "correctness": [
                    {
                        "row_type": "rfps_preservation_probe",
                        "query_id": query_id,
                        "rfps_cancellation_count": values[0],
                        "rfps_threshold_only_count": values[1],
                    }
                    for query_id, values in expected.items()
                ]
            }
        self.assertTrue(reports.rfps_probe_gate(outputs))
        missing = copy.deepcopy(outputs)
        missing[32768]["correctness"].pop()
        self.assertFalse(reports.rfps_probe_gate(missing))
        duplicate = copy.deepcopy(outputs)
        duplicate[32768]["correctness"].append(
            copy.deepcopy(duplicate[32768]["correctness"][0])
        )
        self.assertFalse(reports.rfps_probe_gate(duplicate))
        changed = copy.deepcopy(outputs)
        changed[32768]["correctness"][1]["rfps_cancellation_count"] += 1
        self.assertFalse(reports.rfps_probe_gate(changed))

    def test_thread_gate_rejects_absent_samples(self):
        reports = load("run_benchmark")
        valid = [{"observed_thread_samples": 10, "minimum_threads": 1, "maximum_threads": 1}]
        missing = [{"observed_thread_samples": 0, "minimum_threads": None, "maximum_threads": None}]
        self.assertTrue(reports.thread_observation_gate(valid))
        self.assertFalse(reports.thread_observation_gate(missing))

    def test_immutable_phase2_manifest_matches_current_tree(self):
        reports = load("run_benchmark")
        canonical = json.loads(reports.CANONICAL_INTEGRITY.read_text(encoding="utf-8"))
        self.assertEqual(canonical, reports.integrity_state())

    def test_retained_outputs_are_available_without_a_new_benchmark_run(self):
        reports = load("run_benchmark")
        outputs, processes = reports.load_formal_outputs()
        self.assertEqual(set(outputs), set(reports.formal_workload.EXPECTED))
        self.assertEqual(len(processes), len(reports.formal_workload.EXPECTED))

    def test_verification_uses_existing_test_module(self):
        reports = load("run_benchmark")
        self.assertTrue(reports.verification_test_path().is_file())

    def test_fetch_scaling_tables_share_the_report_directory(self):
        reports = load("run_benchmark")
        outputs, _ = reports.load_formal_outputs()
        with tempfile.TemporaryDirectory() as directory:
            reports.EVIDENCE = Path(directory)
            reports.write_tables(outputs)
            self.assertTrue((reports.EVIDENCE / "11_fetch_k_scaling_raw.csv").is_file())
            self.assertTrue((reports.EVIDENCE / "12_fetch_k_scaling_summary.csv").is_file())


if __name__ == "__main__":
    unittest.main()
