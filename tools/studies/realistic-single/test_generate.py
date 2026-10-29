from pathlib import Path
import tempfile
import unittest

from generate import SOURCE_SHA256, build_study, evaluate


ROOT = Path(__file__).resolve().parents[4]
BGL = ROOT / "docs/project/experiments/search/workloads/source/loghub/BGL_2k.log"
EXPECTED_IDS = [
    146, 160, 167, 189, 212, 218, 317, 401, 409, 486, 560, 567,
    704, 871, 883, 910, 915, 1037, 1117, 1180, 1565, 1622, 1660, 1703,
]


class OracleTests(unittest.TestCase):
    def test_literal_duplicates_and_boundary_are_separate(self):
        records = [
            {"id": 10, "searchable": list(b"ababa"), "payload": list(b"first")},
            {"id": 20, "searchable": list(b"abxx"), "payload": list(b"second")},
        ]
        literal = evaluate(records, b"aba", "fps", None, 0.5)
        self.assertEqual(literal["literal_record_ids"], [10])
        self.assertEqual(literal["literal_valid_positions"], [0, 2])
        self.assertEqual(literal["valid_candidate_record_ids"], [10])
        self.assertEqual(literal["duplicate_candidate_offsets"], 1)
        self.assertEqual(literal["selected_record_id"], 10)

        crossing = evaluate(records, b"ba\0a", "fps", None, 0.5)
        self.assertEqual(crossing["exact_accepted_valid_positions"], [])
        self.assertEqual(crossing["exact_accepted_boundary_positions"], [3])
        self.assertIsNone(crossing["selected_record_id"])

    def test_balanced_rfps_cancellation_is_not_literal(self):
        records = [{"id": 7, "searchable": list(b"ab"), "payload": [1]}]
        result = evaluate(records, b"ba", "rfps", [1, 1], 0.5)
        self.assertEqual(result["literal_valid_positions"], [])
        self.assertEqual(result["exact_accepted_valid_positions"], [0])
        self.assertEqual(result["intrinsic_cancellation_positions"], [0])
        self.assertEqual(result["selected_record_id"], 7)


class BglStudyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.study = build_study(BGL)

    def test_source_and_catalog_are_frozen_before_he(self):
        catalog = self.study["catalog"]
        self.assertEqual(catalog["source"]["sha256"], SOURCE_SHA256)
        self.assertEqual(catalog["source"]["logical_lines"], 2000)
        self.assertEqual(catalog["eligible_records"], 1331)
        self.assertEqual(catalog["selected_record_ids"], EXPECTED_IDS)
        self.assertEqual(catalog["payload_bytes"], 595)
        self.assertEqual(catalog["framed_search_bytes"], 618)
        self.assertEqual(len(catalog["records"]), 24)
        self.assertTrue(all(r["searchable"] == r["payload"] for r in catalog["records"]))

    def test_pattern_families_are_score_blind_and_complete(self):
        patterns = self.study["patterns"]["patterns"]
        self.assertEqual(len(patterns), 13)
        self.assertEqual(
            {p["kind"] for p in patterns},
            {"unique", "multi_record", "excluded_absent", "within_record_repeat",
             "boundary_exact", "one_change", "balanced_change"},
        )
        self.assertEqual(
            sorted(p["length"] for p in patterns if p["kind"] == "unique"),
            [8, 16, 32],
        )
        repeated = next(p for p in patterns if p["kind"] == "within_record_repeat")
        self.assertEqual(bytes(repeated["pattern"]), b" 0x04")
        self.assertGreaterEqual(repeated["selection_oracle"]["literal_occurrences"], 2)
        boundary = next(p for p in patterns if p["kind"] == "boundary_exact")
        self.assertIn(0, boundary["pattern"])
        self.assertEqual(boundary["selection_oracle"]["literal_occurrences"], 0)
        self.assertGreater(boundary["selection_oracle"]["framed_occurrences"], 0)

    def test_case_matrix_has_fixed_denominators_and_paired_weights(self):
        cases = self.study["cases"]
        self.assertEqual(len(cases), 643)
        self.assertEqual(len({case["id"] for case in cases}), 643)
        self.assertEqual(sum(case["mode"] == "fps" for case in cases), 13)
        self.assertEqual(
            sum(case["mode"] == "rfps" and case["weight_policy"] == "uniform_3_32"
                for case in cases),
            624,
        )
        self.assertEqual(
            sum(case["weight_policy"] == "ones_diagnostic" for case in cases), 6
        )
        paired = {}
        for case in cases:
            if case["weight_policy"] != "uniform_3_32":
                continue
            key = (case["pattern_id"], case["weight_draw"])
            paired.setdefault(key, []).append(case)
            self.assertTrue(all(3 <= weight <= 32 for weight in case["weights"]))
        self.assertEqual(len(paired), 13 * 16)
        for group in paired.values():
            self.assertEqual(sorted(case["threshold"] for case in group), [0.5, 1.0, 4.0])
            self.assertEqual(len({tuple(case["weights"]) for case in group}), 1)

    def test_wrong_source_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            wrong = Path(directory) / "BGL_2k.log"
            wrong.write_bytes(BGL.read_bytes()[:-1])
            with self.assertRaisesRegex(ValueError, "source SHA-256"):
                build_study(wrong)


if __name__ == "__main__":
    unittest.main()
