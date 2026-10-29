from pathlib import Path
import tempfile
import unittest

from plots import render


class PlotTests(unittest.TestCase):
    def test_renders_candidate_and_timing_svgs_with_human_labels(self):
        cases = [{
            "case": "rfps_false_competes", "candidate_count": 5,
            "selected_count": 4, "truncated_count": 1,
            "false_selected_count": 1, "boundary_rejected_count": 0,
            "duplicate_count": 0,
        }]
        timings = [{
            "case": "rfps_false_competes", "repetition": 1,
            "match_ns": 100, "locate_ns": 5, "fetch_ns": 40,
            "request_ns": 150,
        }]
        with tempfile.TemporaryDirectory() as directory:
            paths = render(Path(directory), cases, timings)
            self.assertEqual({path.name for path in paths}, {
                "candidate_flow.svg", "stage_timings.svg"
            })
            for path in paths:
                text = path.read_text()
                self.assertIn("rfps false competes", text)
                self.assertIn("<svg", text)
                self.assertGreater(len(text), 500)


if __name__ == "__main__":
    unittest.main()
