import json
from pathlib import Path
import sys
import unittest


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))

from phase2_workload import SOURCE_SHA256, build_all, source_rows


SOURCE = ROOT / "docs/project/experiments/search/workloads/source/loghub/BGL_2k.log"


class Phase2WorkloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = source_rows(SOURCE)
        cls.study = build_all(SOURCE)

    def test_source_and_nested_prefixes_are_frozen(self):
        self.assertEqual(SOURCE_SHA256, "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496")
        self.assertEqual(len(self.rows), 1_331)
        expected = {
            8192: (64, 1629, 2048),
            16384: (128, 3201, 4096),
            32768: (256, 6544, 8192),
        }
        prior_ids = set()
        for ring_dimension, (record_count, text_bytes, transform_len) in expected.items():
            workload = self.study[str(ring_dimension)]["workload"]
            ids = [record["id"] for record in workload["records"]]
            self.assertEqual((workload["n"], workload["text_bytes"], workload["transform_len"]), (record_count, text_bytes, transform_len))
            self.assertEqual(ids, sorted(ids))
            self.assertEqual(len(ids), len(set(ids)))
            self.assertTrue(prior_ids.issubset(ids))
            prior_ids = set(ids)

    def test_real_bytes_and_capacity_constraints_are_not_padded(self):
        for key, study in self.study.items():
            workload = study["workload"]
            ring_dimension = int(key)
            records = workload["records"]
            self.assertTrue(all(record["searchable"] == record["payload"] for record in records))
            self.assertTrue(all(16 <= len(record["payload"]) <= 40 for record in records))
            self.assertEqual(workload["payload_bytes"], sum(len(record["payload"]) for record in records))
            self.assertEqual(workload["text_bytes"], workload["payload_bytes"] + len(records) - 1)
            self.assertLessEqual(workload["transform_len"], ring_dimension // 2)
            self.assertLessEqual(workload["n"] * 128, ring_dimension)
            self.assertEqual(workload["match_utilization"], 0.5)
            self.assertEqual(workload["fetch_window_utilization"], 1.0)
            self.assertEqual(workload["limiting_stage"], "Fetch")

    def test_query_corpus_has_all_required_classes_and_k_values(self):
        required = {
            "fps_absent",
            "fps_within_record_repeat",
            "fps_boundary",
            "rfps_literal",
            "rfps_cancellation",
            "rfps_threshold_only",
        }
        for study in self.study.values():
            queries = study["queries"]
            categories = {query["category"] for query in queries}
            self.assertTrue(required.issubset(categories))
            self.assertEqual({query["public_k"] for query in queries}, {1, 4, 8, 16})
            for public_k in (1, 4, 8, 16):
                relations = {
                    query["candidate_relation"]
                    for query in queries
                    if query["mode"] == "fps" and query["public_k"] == public_k
                }
                self.assertTrue({"fewer", "exact", "more"}.issubset(relations))
            cancellation = next(query for query in queries if query["category"] == "rfps_cancellation")
            threshold = next(query for query in queries if query["category"] == "rfps_threshold_only")
            self.assertGreater(cancellation["oracle"]["intrinsic_cancellation_count"], 0)
            self.assertGreater(threshold["oracle"]["threshold_only_count"], 0)

    def test_generation_is_byte_for_byte_deterministic(self):
        first = json.dumps(self.study, sort_keys=True, separators=(",", ":"))
        second = json.dumps(build_all(SOURCE), sort_keys=True, separators=(",", ":"))
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
