import copy
import contextlib
import csv
import io
import json
from pathlib import Path
import tempfile
import unittest

from analyze import analyze, effects, inspect_run, validate_coverage, summarize
from generate import main as generate, native_trials


def example():
    record = {"id": 10, "searchable": [2, 0], "payload": [10, 0, 255, 0]}
    output = {"id": 10, "payload": record["payload"]}
    inp = {"trial": {"id": "example", "pattern": [1, 1], "weights": [1, 1],
                     "family": "balanced", "policy": "ones_diagnostic", "length": 2},
           "records": [record], "thresholds": [0.5], "layout": "no_match"}
    run = {"scores": [0.02], "output": output, "fetch_count": 1,
           "exact_selection_agreement": True, "semantic_selection_agreement": False,
           "locate": {"accepted_positions": [0], "boundary_rejected_positions": [],
                      "candidates": [{"id": 10, "physical_index": 0, "global_position": 0,
                                      "local_offset": 0, "score": 0.02}],
                      "duplicate_count": 0, "truncated_count": 0,
                      "target": {"Real": {"id": 10, "physical_index": 0}}},
           "oracle": {"windows": [{"id": 10, "local_offset": 0, "global_position": 0,
                                    "exact_score": 0, "semantic_match": False}],
                      "semantic_ids": [], "exact_ids": [10],
                      "semantic_record": None, "exact_record": output},
           "decisions": [{"id": 10, "global_position": 0, "exact_score": 0,
                          "approximate_score": 0.02, "semantic_match": False,
                          "exact_accepted": True, "approximate_accepted": True,
                          "intrinsic_cancellation": True, "threshold_only": False,
                          "ckks_false_positive": False, "ckks_false_negative": False}],
           "timings": {"request_ns": 10, "match_ns": 6, "locate_ns": 1, "fetch_ns": 2,
                       "derived_online_sum_ns": 7, "fetch": {"query_ns": 1, "server_ns": 0, "decrypt_decode_ns": 1}},
           "ckks_metrics": {"openfhe_version": "1.5.1", "phase_timings": {
               "client_query_ns": 1, "server_evaluation_ns": 2, "client_decrypt_idft_ns": 1}}}
    return inp, run


class AnalysisTests(unittest.TestCase):
    def test_effects_are_distinct(self):
        self.assertTrue(effects(0, 0.02, False, 0.5)["intrinsic_cancellation"])
        self.assertTrue(effects(1, 1.01, False, 4)["threshold_only"])
        self.assertTrue(effects(1, 0.49, False, 0.5)["ckks_false_positive"])
        self.assertTrue(effects(0, 0.51, True, 0.5)["ckks_false_negative"])

    def test_negative_selection_is_retained_not_rejected(self):
        inp, run = example()
        rows, outcome = inspect_run(inp, 0.5, run)
        self.assertEqual(outcome, "false_record")
        self.assertEqual(len(rows), 1)
        rates = summarize(rows)
        self.assertEqual(rates[0]["nonmatch_windows"], 1)
        self.assertEqual(rates[0]["false_matches"], 1)

    def test_corrupt_dependent_fields_fail(self):
        for mutate in (
            lambda r: r["output"]["payload"].append(99),
            lambda r: r["locate"]["candidates"][0].update(global_position=9),
            lambda r: r.update(oracle={}),
            lambda r: r["decisions"][0].update(intrinsic_cancellation=False),
            lambda r: r.update(semantic_selection_agreement=True),
            lambda r: r["timings"].update(request_ns=0),
        ):
            inp, run = example()
            run = copy.deepcopy(run)
            mutate(run)
            with self.assertRaises(AssertionError):
                inspect_run(inp, 0.5, run)

    def test_missing_duplicate_or_unbound_attempts_fail(self):
        inp, run = example()
        row = {"schema": "p0d-rfps-native-result-v1", "input": inp,
               "trial_id": "example", "threshold": 0.5, "run": run, "error": None}
        validate_coverage([inp], [row])
        for rows in ([], [row, row], [dict(row, threshold=4)], [dict(row, input={})]):
            with self.assertRaises(AssertionError):
                validate_coverage([inp], rows)

    def test_all_errors_still_emit_attempt_denominators(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1] / "target") as name:
            directory = Path(name)
            generate(directory)
            with (directory / "native_results.jsonl").open("w") as output:
                for inp in native_trials():
                    for threshold in inp["thresholds"]:
                        output.write(json.dumps({"schema": "p0d-rfps-native-result-v1", "input": inp,
                            "trial_id": inp["trial"]["id"], "threshold": threshold,
                            "run": None, "error": "injected harness failure"})+"\n")
            with contextlib.redirect_stdout(io.StringIO()):
                analyze(directory)
            summary = json.loads((directory / "analysis.json").read_text())
            self.assertEqual(summary["native_completed"], 0)
            self.assertEqual(len(summary["execution_errors"]), 1800)
            self.assertEqual(sum(r["attempts"] for r in summary["outcome_counts"]), 1800)
            with (directory / "native_windows.csv").open() as source:
                reader = csv.DictReader(source)
                self.assertIn("exact_score", reader.fieldnames)
                self.assertEqual(list(reader), [])


if __name__ == "__main__":
    unittest.main()
