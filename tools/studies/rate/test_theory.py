from fractions import Fraction
import unittest

from theory import acceptance_fraction, build_report


class TheoryTests(unittest.TestCase):
    def test_balanced_random_weight_probabilities(self):
        weights = range(3, 33)
        self.assertEqual(acceptance_fraction((1, -1), weights, 0.5), Fraction(1, 30))
        self.assertEqual(acceptance_fraction((1, -1), weights, 1), Fraction(22, 225))
        self.assertEqual(acceptance_fraction((1, -1), weights, 4), Fraction(5, 18))

    def test_one_change_and_all_ones_special_cases(self):
        weights = range(3, 33)
        self.assertEqual(acceptance_fraction((1,), weights, 0.5), 0)
        self.assertEqual(acceptance_fraction((1,), weights, 1), 0)
        self.assertEqual(acceptance_fraction((1,), weights, 4), Fraction(1, 15))
        for threshold in (0.5, 1, 4):
            self.assertEqual(acceptance_fraction((1, -1), (1,), threshold), 1)
        self.assertEqual(acceptance_fraction((1,), (1,), 0.5), 0)
        self.assertEqual(acceptance_fraction((1,), (1,), 1), 1)

    def test_invalid_models_fail(self):
        for differences, weights, threshold in (
            ((), range(3, 33), 0.5),
            ((1,), (), 0.5),
            ((1,), range(3, 33), -0.5),
        ):
            with self.assertRaises(ValueError):
                acceptance_fraction(differences, weights, threshold)

    def test_report_aggregates_only_exact_relevant_rows(self):
        base = {"source": "exact", "policy": "uniform_3_32", "family": "balanced",
                "threshold": "0.5"}
        report = build_report([
            {**base, "length": "2", "false_matches": "34", "nonmatch_windows": "1000"},
            {**base, "length": "4", "false_matches": "32", "nonmatch_windows": "1000"},
            {**base, "source": "native", "length": "2",
             "false_matches": "99", "nonmatch_windows": "100"},
            {**base, "family": "full_bytes", "length": "2",
             "false_matches": "99", "nonmatch_windows": "100"},
        ])
        self.assertEqual(report["schema"], "p0d-rfps-analytical-check-v1")
        theory = report["theory"]["balanced/uniform_3_32/0.5"]
        self.assertEqual(theory["accepted_assignments"], 30)
        self.assertEqual(theory["total_assignments"], 900)
        self.assertEqual(theory["fraction"], "1/30")
        self.assertEqual(
            report["observed"]["balanced/uniform_3_32/0.5"],
            {"false_matches": 66, "nonmatch_windows": 2000, "percent": 3.3},
        )


if __name__ == "__main__":
    unittest.main()
