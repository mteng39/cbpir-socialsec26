import csv
from pathlib import Path
import tempfile
import unittest

from plots import render


class PlotTests(unittest.TestCase):
    def test_four_views_retain_plot_source_tables(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            attempts = root / "attempts.csv"
            fields = [
                "case_id", "pattern_id", "pattern_kind", "evidence_class", "mode",
                "weight_policy", "threshold", "approximate_accept_valid",
                "approximate_accept_boundary", "locate_boundary_rejects",
                "locate_candidate_records", "selected_real", "nonliteral_valid_windows",
                "exact_intrinsic_cancellation", "exact_threshold_only", "match_ns",
                "locate_ns", "fetch_ns", "request_ns", "error",
            ]
            with attempts.open("w", newline="") as destination:
                writer = csv.DictWriter(destination, fieldnames=fields)
                writer.writeheader()
                for policy, kind in (("uniform_3_32", "unique"), ("ones_diagnostic", "balanced_change")):
                    for threshold in (0.5, 1.0, 4.0):
                        writer.writerow({
                            "case_id": f"{policy}/{threshold}", "pattern_id": kind,
                            "pattern_kind": kind, "evidence_class": "natural" if kind == "unique" else "controlled",
                            "mode": "rfps", "weight_policy": policy, "threshold": threshold,
                            "approximate_accept_valid": 2, "approximate_accept_boundary": 1,
                            "locate_boundary_rejects": 1, "locate_candidate_records": 1,
                            "selected_real": 1, "nonliteral_valid_windows": 10,
                            "exact_intrinsic_cancellation": 1, "exact_threshold_only": 1,
                            "match_ns": 1_000_000, "locate_ns": 1_000,
                            "fetch_ns": 100_000, "request_ns": 1_200_000, "error": "",
                        })
            multiplicity = root / "multiplicity.csv"
            with multiplicity.open("w", newline="") as destination:
                writer = csv.DictWriter(destination, fieldnames=[
                    "pattern_id", "pattern_kind", "length", "literal_occurrences",
                    "distinct_record_ids", "within_record_duplicate_offsets",
                    "record_occurrence_counts",
                ])
                writer.writeheader()
                writer.writerow({
                    "pattern_id": "repeat", "pattern_kind": "within_record_repeat", "length": 5,
                    "literal_occurrences": 2, "distinct_record_ids": 1,
                    "within_record_duplicate_offsets": 1, "record_occurrence_counts": '{"10": 2}',
                })
            output = root / "plots"
            render(attempts, multiplicity, output)
            for stem in (
                "candidate_funnel", "rfps_false_acceptance_heatmap",
                "pattern_multiplicity", "stage_runtime",
            ):
                self.assertGreater((output / f"{stem}.png").stat().st_size, 1000)
                self.assertGreater((output / f"{stem}.svg").stat().st_size, 1000)
                self.assertGreater((output / f"{stem}.csv").stat().st_size, 20)
                svg_lines = (output / f"{stem}.svg").read_text().splitlines()
                self.assertTrue(all(line == line.rstrip() for line in svg_lines))


if __name__ == "__main__":
    unittest.main()
