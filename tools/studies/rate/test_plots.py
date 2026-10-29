from pathlib import Path
import tempfile
import unittest

from plots import render


class PlotTests(unittest.TestCase):
    def test_renders_four_families_without_inventing_missing_counts(self):
        rate = {"source": "exact", "policy": "uniform_3_32", "family": "balanced",
                "length": 2, "threshold": 0.5, "nonmatch_windows": 10, "false_matches": 2,
                "windows": 10, "intrinsic_cancellation": 2, "threshold_only": 0,
                "ckks_false_positive": 0, "ckks_false_negative": 0}
        window = {"policy": "uniform_3_32", "threshold": 0.5,
                  "exact_score": 0, "approximate_score": 0.1, "semantic_match": False}
        outcome = {"policy": "uniform_3_32", "threshold": 0.5, "layout": "no_match",
                   "attempts": 10, "correct_record": 0, "false_record": 2,
                   "missed_true": 0, "correct_empty": 8, "execution_error": 0}
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1] / "target") as directory:
            paths = render(Path(directory), [rate], [rate], [window], [outcome])
            self.assertEqual(len(paths), 18)
            for path in paths:
                self.assertGreater(path.stat().st_size, 30)
                if path.suffix in (".png", ".svg"):
                    self.assertGreater(path.stat().st_size, 1000)
                if path.suffix == ".png":
                    self.assertEqual(path.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")
            score_svg = (Path(directory) / "score_boundaries.svg").read_text()
            for boundary in ("±0.5", "±1", "±4"):
                self.assertIn(boundary, score_svg)
            for name in ("heatmap_rates.csv", "cause_comparison.csv",
                         "score_boundaries.csv", "k1_outcomes.csv"):
                self.assertGreater((Path(directory) / name).stat().st_size, 30)


if __name__ == "__main__":
    unittest.main()
