import unittest

from analysis import decision_disagreements, independent_windows, validate_fetch


class IndependentAnalysisTests(unittest.TestCase):
    def setUp(self):
        self.records = [
            {"id": 10, "searchable": list(b"ababa"), "payload": list(b"first")},
            {"id": 20, "searchable": list(b"abxx"), "payload": list(b"second")},
        ]

    def test_windows_label_valid_offsets_and_cross_record_offsets(self):
        rows = independent_windows(self.records, b"aba", "fps", [], 0.5)
        accepted = [row for row in rows if row["exact_accepted"]]
        self.assertEqual(
            [(row["global_position"], row["record_id"], row["local_offset"])
             for row in accepted],
            [(0, 10, 0), (2, 10, 2)],
        )
        crossing = independent_windows(self.records, b"ba\0a", "fps", [], 0.5)
        self.assertEqual(
            [row["global_position"] for row in crossing
             if row["exact_accepted"] and row["boundary"]],
            [3],
        )
        self.assertFalse(any(row["literal"] and not row["boundary"] for row in crossing))

    def test_fetch_validation_uses_locate_target_not_match_oracle(self):
        real_trace = {
            "target": {"Real": {"id": 20, "physical_index": 1}},
            "candidates": [{"id": 20}],
        }
        self.assertTrue(
            validate_fetch(self.records, real_trace, {"id": 20, "payload": list(b"second")})
        )
        with self.assertRaisesRegex(ValueError, "payload"):
            validate_fetch(self.records, real_trace, {"id": 20, "payload": list(b"wrong")})

        dummy_trace = {"target": {"Dummy": {"physical_index": 2}}, "candidates": []}
        self.assertTrue(validate_fetch(self.records, dummy_trace, None))
        with self.assertRaisesRegex(ValueError, "dummy"):
            validate_fetch(self.records, dummy_trace, {"id": 10, "payload": list(b"first")})

    def test_ckks_disagreements_keep_valid_and_boundary_counts_separate(self):
        windows = independent_windows(self.records, b"ba\0a", "fps", [], 0.5)
        scores = [float(row["exact_score"]) for row in windows]
        scores[3] = 2.0
        counts = decision_disagreements(windows, scores, 0.5)
        self.assertEqual(counts["ckks_false_negative_boundary"], 1)
        self.assertEqual(counts["ckks_false_negative_valid"], 0)
        nonaccepted_boundary = next(
            row["global_position"] for row in windows
            if row["boundary"] and not row["exact_accepted"]
        )
        scores[nonaccepted_boundary] = 0.0
        counts = decision_disagreements(windows, scores, 0.5)
        self.assertEqual(counts["ckks_false_positive_boundary"], 1)


if __name__ == "__main__":
    unittest.main()
