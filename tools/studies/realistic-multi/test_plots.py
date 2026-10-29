from pathlib import Path
import tempfile
import unittest

from plots import render


class PlotTests(unittest.TestCase):
    def test_renders_four_svg_views(self):
        attempts = [{
            "case_id": "x", "mode": "rfps", "candidate_count": 5,
            "selected_count": 4, "false_selected_count": 1,
            "boundary_rejected_count": 2, "duplicate_count": 3,
            "truncated_count": 1, "weight_policy": "uniform_3_32",
            "threshold": 0.5,
        }]
        distribution = [{"candidate_size": 5, "requests": 1}]
        timings = [{
            "case_id": "x", "match_ns": 100, "locate_ns": 2,
            "fetch_ns": 40, "request_ns": 145,
        }]
        with tempfile.TemporaryDirectory() as directory:
            paths = render(Path(directory), attempts, distribution, timings)
            self.assertEqual(len(paths), 4)
            for path in paths:
                text = path.read_text()
                self.assertIn("<svg", text)
                self.assertGreater(len(text), 500)


if __name__ == "__main__":
    unittest.main()
