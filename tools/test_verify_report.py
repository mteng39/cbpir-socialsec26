"""Check that the retained-report verifier rejects altered evidence.

Run with the full report and its separate CKKS size report as arguments.
"""
import copy
import json
from pathlib import Path
import sys
import unittest

from verify_report import validate


class ReportVerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report = json.loads(Path(sys.argv[1]).read_text())
        cls.sizes = json.loads(Path(sys.argv[2]).read_text())

    def test_retained_report(self):
        self.assertEqual(validate(self.report, self.sizes)["validation"], "passed")

    def reject(self, mutate):
        report = copy.deepcopy(self.report)
        sizes = copy.deepcopy(self.sizes)
        mutate(report, sizes)
        with self.assertRaises(AssertionError):
            validate(report, sizes)

    def test_wrong_payload(self):
        self.reject(lambda r, s: r["cases"][0]["run"]["output"]["payload"].append(1))

    def test_wrong_candidate(self):
        self.reject(lambda r, s: r["cases"][0]["run"]["locate"]["candidates"][0].update(id=999))

    def test_missing_case(self):
        self.reject(lambda r, s: r["cases"].pop())

    def test_hidden_cancellation(self):
        self.reject(lambda r, s: r["cases"][9]["run"]["decisions"][0].update(intrinsic_cancellation=False))

    def test_impossible_request_duration(self):
        self.reject(lambda r, s: r["cases"][0]["run"]["timings"].update(request_ns=0))

    def test_wrong_statistic(self):
        self.reject(lambda r, s: r["performance"][0]["statistics"]["match"].update(median_ns=0))

    def test_wrong_probe_runtime(self):
        self.reject(lambda r, s: s["probes"][0].update(openfhe_version="0.0.0"))

    def test_failed_bgv_roundtrip(self):
        self.reject(lambda r, s: r["serialization"]["bgv"].update(roundtrip_exact=False))

    def test_missing_performance(self):
        self.reject(lambda r, s: r.update(performance=[]))

    def test_missing_oracle(self):
        self.reject(lambda r, s: r["cases"][0]["run"].update(oracle={}))

    def test_wrong_selection_agreement(self):
        self.reject(lambda r, s: r["cases"][0]["run"].update(exact_selection_agreement=False))

    def test_wrong_decision_acceptance(self):
        self.reject(lambda r, s: r["cases"][0]["run"]["decisions"][0].update(exact_accepted=False))

    def test_wrong_decision_score(self):
        self.reject(lambda r, s: r["cases"][0]["run"]["decisions"][0].update(approximate_score=999))

    def test_wrong_candidate_offset(self):
        self.reject(lambda r, s: r["cases"][0]["run"]["locate"]["candidates"][0].update(local_offset=99))

    def test_wrong_warmup_output(self):
        self.reject(lambda r, s: r["performance"][0]["warmup"].update(output=None))

    def test_wrong_repetition_output(self):
        self.reject(lambda r, s: r["performance"][0]["repetitions"][0].update(output=None))

    def test_duplicate_probe(self):
        self.reject(lambda r, s: s.update(probes=[s["probes"][0], s["probes"][0]]))

    def test_wrong_probe_positions(self):
        self.reject(lambda r, s: s["probes"][0].update(matching_positions=[999]))

    def test_wrong_probe_text(self):
        self.reject(lambda r, s: s["probes"][0]["text"].append(1))

    def test_wrong_probe_request(self):
        self.reject(lambda r, s: s["probes"][0]["request"].update(threshold=999))

    def test_wrong_fixture_name(self):
        self.reject(lambda r, s: r["cases"][0]["fixture"].update(name="unrecognized"))


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]], verbosity=2)
