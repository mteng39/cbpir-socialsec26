import importlib.util
import json
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "search_correctness_v2", HERE / "run.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SearchCorrectnessV2Tests(unittest.TestCase):
    def test_pattern_selection_uses_support_then_earliest_occurrence(self):
        records = []
        for record_id in range(1, 18):
            records.append({"id": record_id, "searchable": list(b"aaax")})
        for record_id in range(18, 33):
            records.append({"id": record_id, "searchable": list(b"bbby")})

        candidates = MODULE.enumerate_natural_patterns(records, 4)
        selected = MODULE.select_natural_pattern(candidates, 4)

        self.assertEqual(selected["pattern"], b"aaax")
        self.assertEqual(len(selected["record_ids"]), 17)
        self.assertEqual(len(selected["alignment_positions"]), 17)

    def test_single_byte_selection_preserves_original_frozen_pattern(self):
        records = [
            {"id": 1, "searchable": list(b"ab")},
            {"id": 2, "searchable": list(b"ac")},
        ]

        candidates = MODULE.enumerate_natural_patterns(records, 1)
        selected = MODULE.select_natural_pattern(
            candidates, 1, original_pattern=b"c"
        )

        self.assertEqual(selected["pattern"], b"c")

    def test_pattern_selection_rejects_missing_original_pattern(self):
        candidates = MODULE.enumerate_natural_patterns(
            [{"id": 1, "searchable": list(b"ab")}], 1
        )

        with self.assertRaisesRegex(ValueError, "original pattern"):
            MODULE.select_natural_pattern(candidates, 1, original_pattern=b"z")

    def test_searchable_length_range_is_recomputed_from_records(self):
        records = [
            {"id": 1, "searchable": [1, 2, 3]},
            {"id": 2, "searchable": [4, 5, 6, 7, 8]},
        ]

        self.assertEqual(MODULE.searchable_length_range(records), (3, 5))

    def test_pattern_escape_is_unambiguous(self):
        self.assertEqual(MODULE.escape_pattern(b"a\x00\\\xff"), r"a\x00\\\xff")

    def test_query_fixture_builder_emits_all_approved_lengths(self):
        records = [
            {"id": record_id, "searchable": list(b"abcdefghijklmnopqr")}
            for record_id in range(1, 17)
        ]
        source = {
            "profile": "test",
            "timing_fixture": {
                "source_query_id": "original",
                "pattern": [97],
                "threshold": 0.5,
            },
            "workload": {"records": records},
        }

        fixtures, support_rows = MODULE.build_query_fixtures(source)

        self.assertEqual([len(fixture["pattern"]) for fixture in fixtures], [1, 4, 8, 16])
        self.assertEqual(fixtures[0]["source_query_id"], "original")
        self.assertEqual(fixtures[0]["pattern"], [97])
        self.assertEqual(fixtures[-1]["expected_literal_record_count"], 16)
        self.assertTrue(any(row["pattern_length"] == 16 for row in support_rows))

    def test_literal_enumeration_stays_inside_records_and_deduplicates_ids(self):
        records = [
            {"id": 10, "searchable": [97, 98, 97, 98]},
            {"id": 20, "searchable": [97, 98]},
        ]
        positions, ids = MODULE.enumerate_literal(records, b"ab")
        self.assertEqual(positions, [0, 2, 5])
        self.assertEqual(ids, [10, 20])

    def test_record_metrics_derive_counts_from_sets(self):
        metrics = MODULE.record_metrics([1, 2, 3], [1, 3, 4])
        self.assertEqual(metrics["true_positive_ids"], [1, 3])
        self.assertEqual(metrics["false_negative_ids"], [2])
        self.assertEqual(metrics["false_positive_ids"], [4])
        self.assertEqual((metrics["tp"], metrics["fn"], metrics["fp"]), (2, 1, 1))

    def test_exact_rfps_enumeration_detects_signed_cancellation(self):
        records = [{"id": 7, "searchable": [98, 96]}]
        positions, ids, cancellations, threshold_only = MODULE.enumerate_exact_rfps(
            records, b"aa", [3, 3], 0.5
        )
        self.assertEqual(positions, [0])
        self.assertEqual(ids, [7])
        self.assertEqual(cancellations, [0])
        self.assertEqual(threshold_only, [])

    def test_aggregate_uses_total_record_instances(self):
        rows = [
            {"tp": 2, "fn_count": 1, "fp_count": 1},
            {"tp": 1, "fn_count": 2, "fp_count": 0},
        ]
        aggregate = MODULE.aggregate_counts(rows)
        self.assertEqual((aggregate["tp"], aggregate["gt"], aggregate["fn"], aggregate["fp"]), (3, 6, 3, 1))
        self.assertEqual(aggregate["recall"], 0.5)
        self.assertEqual(aggregate["precision"], 0.75)

    def test_seed_schedule_matches_retained_formal_matrix(self):
        schedule = MODULE.formal_seed_schedule(8192)
        self.assertEqual(len(schedule), 80)
        self.assertEqual(
            (schedule[0]["source_public_k"], schedule[0]["source_run"], schedule[0]["invocation"]),
            (1, 0, 3),
        )
        self.assertEqual(
            (schedule[20]["source_public_k"], schedule[20]["source_run"], schedule[20]["invocation"]),
            (4, 0, 26),
        )
        self.assertEqual(MODULE.fresh_rfps_weights(schedule[0]["seed"], 1), [10])
        self.assertEqual(
            MODULE.fresh_rfps_weights(schedule[0]["seed"], 1),
            MODULE.fresh_rfps_weights(schedule[0]["seed"], 16)[:1],
        )

    def test_performance_snapshot_excludes_correctness_cells(self):
        latex = r"""
        \label{tab:performance}
        & CBPIR-FPS & 1.0 & 2.0 & 3.0 & 4.0 & 5.0
          & 6.0 & 0 & 7.0 & 8.0 & 20/20 & 20/20 \\
        \end{tabular}
        """
        self.assertEqual(
            MODULE.extract_performance_core_rows(latex),
            [{"protocol": "CBPIR-FPS", "core_cells": ["1.0", "2.0", "3.0", "4.0", "5.0", "6.0", "0", "7.0", "8.0"]}],
        )

    def test_public_table_snapshot_verification_does_not_require_manuscript(self):
        manuscript = MODULE.ROOT / "article/cbpir_main.tex"
        before = json.loads(
            (MODULE.EVIDENCE / "validation/09_table2_core_before.json").read_text(encoding="utf-8")
        )

        self.assertFalse(manuscript.exists())
        self.assertEqual(MODULE.verify_table2_snapshot(), len(before))

    def test_frozen_manifest_excludes_generated_python_caches(self):
        manifest = (MODULE.EVIDENCE / "validation/frozen_hashes_before.sha256").read_text(
            encoding="utf-8"
        )

        self.assertNotIn("__pycache__", manifest)
        self.assertNotIn(".pyc", manifest)

    def test_native_build_is_serialized_for_the_memory_bounded_host(self):
        self.assertEqual(MODULE.benchmark_env()["CARGO_BUILD_JOBS"], "1")

    def test_range_expansion_triggers_frozen_evidence_review(self):
        rows = [
            {"old_min": 10, "old_max": 16, "new_min": 5, "new_max": 16},
            {"old_min": 0, "old_max": 17, "new_min": 6, "new_max": 17},
        ]

        self.assertTrue(MODULE.exceeds_retained_candidate_range(rows))
        self.assertFalse(
            MODULE.exceeds_retained_candidate_range(
                [{"old_min": 0, "old_max": 17, "new_min": 6, "new_max": 17}]
            )
        )

    def test_batch_normalization_marks_output_and_every_raw_row(self):
        output = {"runs": [{"repetition": 0}, {"repetition": 1}]}

        normalized = MODULE.normalize_batch_output(output, 3)

        self.assertEqual(normalized["batch_id"], 3)
        self.assertEqual([row["batch_id"] for row in normalized["runs"]], [3, 3])
        self.assertNotIn("batch_id", output)

    def test_bootstrap_is_deterministic_and_resamples_whole_runs(self):
        rows = [
            {"tp": 10, "fn_count": 0, "fp_count": 0},
            {"tp": 0, "fn_count": 10, "fp_count": 10},
        ]

        first = MODULE.bootstrap_cell(rows, resamples=10_000, seed=1234)
        second = MODULE.bootstrap_cell(rows, resamples=10_000, seed=1234)

        self.assertEqual(first, second)
        self.assertEqual(first["recall_low"], 0.0)
        self.assertEqual(first["recall_high"], 1.0)
        self.assertEqual(first["precision_low"], 0.0)
        self.assertEqual(first["precision_high"], 1.0)

    def test_batch_summaries_keep_each_batch_separate(self):
        rows = [
            {"batch_id": 1, "tp": 2, "fn_count": 0, "fp_count": 1,
             "ckks_candidate_record_ids": [1, 2, 3]},
            {"batch_id": 1, "tp": 1, "fn_count": 1, "fp_count": 0,
             "ckks_candidate_record_ids": [1]},
            {"batch_id": 2, "tp": 2, "fn_count": 0, "fp_count": 0,
             "ckks_candidate_record_ids": [1, 2]},
        ]

        summaries = MODULE.summarize_batches(rows)

        self.assertEqual([row["batch_id"] for row in summaries], [1, 2])
        self.assertEqual(
            (summaries[0]["TP"], summaries[0]["GT"], summaries[0]["FN"], summaries[0]["FP"]),
            (3, 4, 1, 1),
        )
        self.assertEqual(
            (
                summaries[0]["candidate_count_min"],
                summaries[0]["candidate_count_median"],
                summaries[0]["candidate_count_max"],
            ),
            (1, 2.0, 3),
        )


if __name__ == "__main__":
    unittest.main()
