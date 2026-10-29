#!/usr/bin/env python3
"""Tests for the paper-only BF-COIE structural model.

These tests do not implement BF-COIE and do not contain timing constants.
"""

from __future__ import annotations

import importlib.util
import math
from pathlib import Path
import unittest


MODEL_PATH = Path(__file__).with_name("03_structural_model.py")
SPEC = importlib.util.spec_from_file_location("bfcoie_structural_model", MODEL_PATH)
assert SPEC is not None and SPEC.loader is not None
MODEL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODEL)


class ControlledMatrixTests(unittest.TestCase):
    def test_power_of_two_hierarchy_and_protocol_invariants(self) -> None:
        expected_t = {
            128: {1: 6, 4: 4, 8: 3, 16: 2},
            256: {1: 7, 4: 5, 8: 4, 16: 3},
            512: {1: 8, 4: 6, 8: 5, 16: 4},
        }
        for n, by_s in expected_t.items():
            for s, expected in by_s.items():
                with self.subTest(n=n, s=s):
                    result = MODEL.build_model(n=n, s=s, fp=16, eta=2, lambda_bits=40)
                    self.assertEqual(result.hierarchy_t, expected)
                    self.assertEqual(result.bf_levels, expected + 1)
                    self.assertEqual(result.pir_request_count, s + 16)
                    self.assertEqual(result.ct_ct_multiplications, 0)
                    self.assertEqual(result.count_reduction_additions, n - 1)
                    self.assertEqual(
                        result.encode_hash_evaluations,
                        2 * n * (expected + 1),
                    )
                    self.assertEqual(
                        result.semantic_domain,
                        "structurally supported but outside original experimental range",
                    )

    def test_invalid_sparsity_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "s must not exceed n"):
            MODEL.build_model(n=8, s=9)
        with self.assertRaisesRegex(ValueError, "2s must not exceed n"):
            MODEL.build_model(n=16, s=16)


class BloomFilterDimensionTests(unittest.TestCase):
    def test_paper_point_rounding_candidates(self) -> None:
        floor_result = MODEL.build_model(
            n=16384,
            s=16,
            fp=16,
            eta=2,
            lambda_bits=40,
            ell_rounding="floor",
        )
        nearest_result = MODEL.build_model(
            n=16384,
            s=16,
            fp=16,
            eta=2,
            lambda_bits=40,
            ell_rounding="nearest",
        )
        ceil_result = MODEL.build_model(
            n=16384,
            s=16,
            fp=16,
            eta=2,
            lambda_bits=40,
            ell_rounding="ceil",
        )
        expected_raw = 2 * 16 * math.sqrt(48)
        self.assertAlmostEqual(floor_result.ell_raw, expected_raw)
        self.assertEqual(floor_result.ell_integer, 221)
        self.assertEqual(nearest_result.ell_integer, 222)
        self.assertEqual(ceil_result.ell_integer, 222)

    def test_exact_minimum_is_distinct_from_paper_sufficient_formula(self) -> None:
        result = MODEL.build_model(n=128, s=16, fp=16, eta=2, lambda_bits=40)
        self.assertLess(result.ell_exact_inequality_minimum, result.ell_integer)
        self.assertTrue(MODEL.bloom_false_positive_bound_holds(result.ell_integer, 16, 2, 48))
        self.assertTrue(
            MODEL.bloom_false_positive_bound_holds(
                result.ell_exact_inequality_minimum,
                16,
                2,
                48,
            )
        )
        self.assertFalse(
            MODEL.bloom_false_positive_bound_holds(
                result.ell_exact_inequality_minimum - 1,
                16,
                2,
                48,
            )
        )


class HierarchyTests(unittest.TestCase):
    def test_section_4_2_example_projection(self) -> None:
        selected = {1, 15, 16}
        expected = {
            4: {1},
            3: {1, 2},
            2: {1, 4},
            1: {1, 8},
            0: {1, 15, 16},
        }
        self.assertEqual(MODEL.projected_index_sets(selected, max_level=4), expected)

    def test_non_power_of_two_requires_explicit_policy(self) -> None:
        with self.assertRaisesRegex(ValueError, "explicit non-power-of-two hierarchy convention"):
            MODEL.build_model(n=10000, s=16)

        floor_result = MODEL.build_model(
            n=10000,
            s=16,
            hierarchy_convention="floor",
        )
        ceil_result = MODEL.build_model(
            n=10000,
            s=16,
            hierarchy_convention="ceil",
        )
        padded_result = MODEL.build_model(
            n=10000,
            s=16,
            hierarchy_convention="pad_to_power_of_two",
        )
        self.assertEqual((floor_result.hierarchy_t, floor_result.bf_levels), (8, 9))
        self.assertEqual((ceil_result.hierarchy_t, ceil_result.bf_levels), (9, 10))
        self.assertEqual((padded_result.hierarchy_t, padded_result.bf_levels), (9, 10))
        self.assertEqual(padded_result.effective_n, 16384)

    def test_1323_is_not_reconciled_by_documented_candidates(self) -> None:
        counts = MODEL.paper_figure5_count_candidates(n=10000, s=16, fp=16, eta=2)
        self.assertNotIn(1323, counts.values())
        self.assertIn(1998, counts.values())
        self.assertIn(1999, counts.values())
        self.assertIn(2220, counts.values())
        self.assertIn(2221, counts.values())


if __name__ == "__main__":
    unittest.main(verbosity=2)
