import json
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]

from final_workload import ALLOWED_WIDTHS, build_all, build_window_matrix, pool_audit


SOURCE = ROOT / "docs/project/experiments/search/workloads/source/loghub/BGL_2k.log"


class FinalFixedWorkloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.audit = pool_audit(SOURCE)
        cls.matrix = build_window_matrix(SOURCE, cls.audit["required_slots_by_id"])
        cls.studies = build_all(SOURCE, cls.audit["required_slots_by_id"])

    def test_complete_pool_uses_real_frame_prefix_sizes(self):
        self.assertEqual(self.audit["eligible_record_count"], 1331)
        self.assertEqual(self.audit["raw_searchable_min"], 17)
        self.assertEqual(self.audit["raw_searchable_max"], 40)
        self.assertEqual(self.audit["raw_payload_min"], 17)
        self.assertEqual(self.audit["raw_payload_max"], 40)
        self.assertEqual(self.audit["serialized_slots_min"], 31)
        self.assertEqual(self.audit["serialized_slots_max"], 54)
        self.assertEqual(self.audit["serialized_slots_median"], 35)
        self.assertTrue(self.audit["all_fit_64"])
        self.assertEqual(self.audit["widths_fitting_all"], list(ALLOWED_WIDTHS))

    def test_every_ring_width_pair_has_actual_nested_prefix_measurement(self):
        self.assertEqual(len(self.matrix), 18)
        expected = {
            (8192, 64): (128, 3201, 4096),
            (8192, 128): (64, 1629, 2048),
            (8192, 224): (36, 925, 1024),
            (8192, 256): (32, 829, 1024),
            (8192, 384): (21, 547, 1024),
            (8192, 512): (16, 416, 512),
            (16384, 64): (256, 6544, 8192),
            (16384, 128): (128, 3201, 4096),
            (16384, 224): (73, 1845, 2048),
            (16384, 256): (64, 1629, 2048),
            (16384, 384): (42, 1098, 2048),
            (16384, 512): (32, 829, 1024),
            (32768, 64): (512, 13358, 16384),
            (32768, 128): (256, 6544, 8192),
            (32768, 224): (146, 3669, 4096),
            (32768, 256): (128, 3201, 4096),
            (32768, 384): (85, 2145, 4096),
            (32768, 512): (64, 1629, 2048),
        }
        actual = {
            (row["N"], row["w"]): (row["n"], row["text_bytes"], row["L"])
            for row in self.matrix
        }
        self.assertEqual(actual, expected)

    def test_smallest_valid_width_rule_freezes_full_both_view_profiles(self):
        expected = {
            "8192": (128, 64, 3201, 4096),
            "16384": (256, 64, 6544, 8192),
            "32768": (512, 64, 13358, 16384),
        }
        for key, values in expected.items():
            workload = self.studies[key]["workload"]
            self.assertEqual(
                (
                    workload["n"],
                    workload["record_slots"],
                    workload["text_bytes"],
                    workload["transform_len"],
                ),
                values,
            )
            self.assertEqual(workload["match_utilization"], 1.0)
            self.assertEqual(workload["fetch_window_utilization"], 1.0)
            self.assertEqual(workload["limiting_stage"], "Fetch")
            self.assertEqual(workload["max_serialized_slots"], 54)

    def test_queries_are_regenerated_and_cover_retained_classes(self):
        required = {
            "fps_absent",
            "fps_multiplicity",
            "fps_within_record_repeat",
            "fps_boundary",
            "rfps_literal",
            "rfps_cancellation",
            "rfps_threshold_only",
        }
        for study in self.studies.values():
            queries = study["queries"]
            self.assertTrue(required.issubset({query["category"] for query in queries}))
            self.assertEqual({query["public_k"] for query in queries}, {1, 4, 8, 16})

    def test_generation_is_byte_for_byte_deterministic(self):
        first = json.dumps(self.studies, sort_keys=True, separators=(",", ":"))
        second = json.dumps(
            build_all(SOURCE, self.audit["required_slots_by_id"]),
            sort_keys=True,
            separators=(",", ":"),
        )
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
