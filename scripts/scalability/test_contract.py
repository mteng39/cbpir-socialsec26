#!/usr/bin/env python3

import importlib.util
import csv
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "run.py"


def load_module():
    spec = importlib.util.spec_from_file_location("scalability_v1", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ScalabilityContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_module()

    def test_extended_modulus_supports_all_planned_rings(self):
        modulus = self.module.PLAINTEXT_MODULUS
        self.assertEqual(786_433, modulus)
        for ring in (32_768, 65_536, 131_072):
            self.assertEqual(0, (modulus - 1) % (2 * ring))

    def test_matrix_has_control_and_larger_profiles_in_separate_processes(self):
        matrix = self.module.expected_matrix()
        self.assertEqual(4, len(matrix))
        self.assertEqual({512, 1024}, {row["n"] for row in matrix})
        self.assertEqual({"fps", "rfps"}, {row["mode"] for row in matrix})
        self.assertTrue(all(row["public_k"] == 16 for row in matrix))
        self.assertTrue(all(row["warmups"] == 3 for row in matrix))
        self.assertTrue(all(row["measured_runs"] == 20 for row in matrix))
        self.assertEqual(80, sum(row["measured_runs"] for row in matrix))

    def test_pinned_corpus_supports_1024_but_not_2048(self):
        audit = self.module.corpus_audit()
        self.assertEqual(1331, audit["eligible_records"])
        self.assertTrue(audit["supports_1024"])
        self.assertFalse(audit["supports_2048"])
        self.assertIn("1331", audit["n2048_status"])

    def test_workloads_preserve_control_and_measure_actual_scale(self):
        control = self.module.build_workload(512, 32_768)
        larger = self.module.build_workload(1024, 65_536)
        self.assertEqual(13_358, control["text_bytes"])
        self.assertEqual(16_384, control["transform_len"])
        self.assertEqual(26_524, larger["text_bytes"])
        self.assertEqual(32_768, larger["transform_len"])
        self.assertEqual(64, control["record_slots"])
        self.assertEqual(64, larger["record_slots"])
        self.assertEqual(512 * 64, control["fetch_slots_used"])
        self.assertEqual(1024 * 64, larger["fetch_slots_used"])
        self.assertTrue(
            {record["id"] for record in control["records"]}.issubset(
                {record["id"] for record in larger["records"]}
            )
        )

    def test_fixed_query_is_shared_and_has_at_least_k_candidates(self):
        for n, ring, expected in ((512, 32_768, 17), (1024, 65_536, 36)):
            workload = self.module.build_workload(n, ring)
            fixture = self.module.build_query_fixture(workload)
            self.assertEqual([ord(":")], fixture["pattern"])
            self.assertEqual(expected, fixture["expected_literal_record_count"])
            self.assertGreaterEqual(fixture["expected_literal_record_count"], 16)

    def test_native_input_carries_explicit_he_parameters(self):
        payload = self.module.build_native_input(1024, 65_536, "fps")
        self.assertEqual("cbpir-scalability-v1-input", payload["schema"])
        self.assertEqual(["fps"], payload["modes"])
        self.assertEqual(65_536, payload["he_parameters"]["ckks_ring_dimension"])
        self.assertEqual(32_768, payload["he_parameters"]["ckks_batch_size"])
        self.assertEqual(65_536, payload["he_parameters"]["bgv_ring_dimension"])
        self.assertEqual(786_433, payload["he_parameters"]["bgv_plaintext_modulus"])

    def test_native_temp_directory_avoids_system_tmpfs(self):
        environment = self.module.benchmark_env()
        temporary = Path(environment["TMPDIR"])
        self.assertTrue(temporary.is_dir())
        self.assertNotEqual(Path("/tmp"), temporary)
        self.assertTrue(str(temporary).startswith(str(self.module.TARGET)))

    def test_retained_evidence_has_complete_validated_matrix(self):
        evidence = self.module.ROOT / "evaluation/scalability-v1"
        with (evidence / "raw.csv").open(newline="", encoding="utf-8") as handle:
            raw = list(csv.DictReader(handle))
        with (evidence / "summary.csv").open(newline="", encoding="utf-8") as handle:
            summary = list(csv.DictReader(handle))
        self.assertEqual(80, len(raw))
        self.assertEqual(4, len(summary))
        self.assertEqual({"512", "1024"}, {row["n"] for row in raw})
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
