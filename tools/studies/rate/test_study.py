import unittest

from generate import LENGTHS, POLICIES, exact_score, iter_plain_trials, make_trial, native_trials


class WorkloadTests(unittest.TestCase):
    def test_hand_scores(self):
        self.assertEqual(exact_score([2, 0], [1, 1], [1, 1]), 0)
        self.assertEqual(exact_score([2, 0], [1, 1], [3, 4]), -1)

    def test_reproducibility_and_policy_pairing(self):
        a = make_trial("balanced", 8, 42, POLICIES[0])
        self.assertEqual(a, make_trial("balanced", 8, 42, POLICIES[0]))
        b = make_trial("balanced", 8, 42, POLICIES[1])
        self.assertEqual(a["text"], b["text"])
        self.assertEqual(a["pattern"], b["pattern"])
        self.assertTrue(all(3 <= w <= 32 for w in a["weights"]))
        self.assertEqual(b["weights"], [1]*8)
        self.assertEqual(b["exact_score"], 0)

    def test_matrix_and_native_coverage(self):
        trials = list(iter_plain_trials())
        self.assertEqual(len(trials), 48000)
        self.assertEqual(len({t["id"] for t in trials}), 48000)
        self.assertTrue(all(t["text"] != t["pattern"] for t in trials))
        self.assertEqual({t["length"] for t in trials}, set(LENGTHS))
        native = list(native_trials())
        self.assertEqual(len(native), 600)
        self.assertEqual(sum(len(t["thresholds"]) for t in native), 1800)
        self.assertEqual(sum(t["trial"]["family"] == "true_match" for t in native), 120)
        ids = {t["id"] for t in trials}
        self.assertTrue(all(t["trial"]["id"] in ids for t in native if t["trial"]["family"] != "true_match"))


if __name__ == "__main__":
    unittest.main()
