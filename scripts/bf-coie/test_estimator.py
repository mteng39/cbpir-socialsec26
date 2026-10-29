#!/usr/bin/env python3
"""Contract tests for paper-calibrated BF-COIE estimates."""

from __future__ import annotations

import unittest
import csv
import math
from pathlib import Path
from tempfile import TemporaryDirectory

from estimate import (
    FIG4_RETRIEVAL_SECONDS,
    PAPER_S,
    build_cells,
    model_fits,
    structural_cell,
    write_generated_outputs,
)


class StructuralTests(unittest.TestCase):
    def test_exact_matrix_and_workload_normalization(self) -> None:
        cells = build_cells()
        self.assertEqual(len(cells), 12)
        self.assertEqual({cell.n for cell in cells}, {128, 256, 512})
        self.assertEqual({cell.s for cell in cells}, {1, 4, 8, 16})
        for cell in cells:
            self.assertEqual(cell.logical_item_bytes, 64)
            self.assertEqual(cell.database_bytes, cell.n * 64)
            self.assertEqual(cell.pir_requests, cell.s + 16)
            self.assertEqual(cell.ct_ct_multiplications, 0)
            self.assertTrue(cell.fairly_comparable)

    def test_accepted_structural_examples(self) -> None:
        smallest = structural_cell(128, 1)
        self.assertEqual(smallest.hierarchy_t, 6)
        self.assertEqual(smallest.bf_levels, 7)
        self.assertEqual(smallest.m, 33)
        self.assertEqual(smallest.ell, 12)
        self.assertEqual(smallest.compactness, 84)
        self.assertEqual(smallest.count_additions, 127)
        self.assertEqual(smallest.encode_hashes, 1792)

        largest = structural_cell(512, 16)
        self.assertEqual(largest.hierarchy_t, 4)
        self.assertEqual(largest.bf_levels, 5)
        self.assertEqual(largest.m, 48)
        self.assertEqual(largest.ell, 222)
        self.assertEqual(largest.compactness, 1110)
        self.assertEqual(largest.pir_requests, 32)

    def test_estimates_have_provenance_and_valid_ranges(self) -> None:
        for cell in build_cells():
            self.assertGreater(cell.locate_ms, 0)
            self.assertGreater(cell.fetch_ms, 0)
            self.assertLessEqual(cell.locate_low_ms, cell.locate_ms)
            self.assertGreaterEqual(cell.locate_high_ms, cell.locate_ms)
            self.assertLessEqual(cell.fetch_low_ms, cell.fetch_ms)
            self.assertGreaterEqual(cell.fetch_high_ms, cell.fetch_ms)
            self.assertIn("F", cell.locate_provenance)
            self.assertIn("X", cell.locate_provenance)
            self.assertIn("F", cell.fetch_provenance)
            self.assertIn("X", cell.fetch_provenance)

    def test_locate_central_uses_floor_hierarchy_origin_fit(self) -> None:
        fits = model_fits()
        server_slope = fits["server_floor_origin"][0]
        client_slope = fits["client_floor_origin"][0]
        for cell in build_cells():
            structure = structural_cell(cell.n, cell.s)
            expected_ms = 1000 * (
                server_slope * structure.encode_hashes
                + client_slope * structure.compactness
            )
            self.assertAlmostEqual(cell.locate_ms, expected_ms)
            self.assertEqual(cell.locate_central_model, "floor-hierarchy through-origin")

    def test_locate_sensitivity_uses_origin_hierarchy_models_only(self) -> None:
        fits = model_fits()
        for cell in build_cells():
            structure = structural_cell(cell.n, cell.s)
            alternatives = []
            for hierarchy in ("floor", "ceil"):
                alternatives.append(
                    1000
                    * (
                        fits[f"server_{hierarchy}_origin"][0]
                        * structure.encode_hashes
                        + fits[f"client_{hierarchy}_origin"][0]
                        * structure.compactness
                    )
                )
            self.assertAlmostEqual(cell.locate_low_ms, min(alternatives))
            self.assertAlmostEqual(cell.locate_high_ms, max(alternatives))

    def test_fetch_central_uses_intercept_fit_below_measured_s_range(self) -> None:
        slope, offset, _ = model_fits()["fetch_intercept"]
        for cell in build_cells():
            geometry_factor = 15 / 14 if cell.n == 512 else 1
            if cell.s in (1, 4):
                expected_ms = 1000 * (slope * (cell.s + 16) + offset) * geometry_factor
                self.assertAlmostEqual(cell.fetch_ms, expected_ms)
                self.assertEqual(
                    cell.fetch_central_model,
                    "request-count intercept fit",
                )
            else:
                expected_ms = (
                    1000
                    * FIG4_RETRIEVAL_SECONDS[PAPER_S.index(cell.s)]
                    * geometry_factor
                )
                self.assertAlmostEqual(cell.fetch_ms, expected_ms)
                self.assertEqual(
                    cell.fetch_central_model,
                    "Figure 4 Retrieval anchor",
                )

    def test_n512_fetch_factor_is_central_with_parameter_sensitivity(self) -> None:
        for cell in build_cells():
            if cell.n == 512:
                self.assertAlmostEqual(cell.fetch_geometry_factor, 15 / 14)
                self.assertEqual(cell.fetch_geometry_factor_low, 1.0)
                self.assertEqual(cell.fetch_geometry_factor_high, 1.5)
                self.assertEqual(
                    cell.fetch_geometry_decision,
                    "retain 15/14 with parameter sensitivity",
                )
            else:
                self.assertEqual(cell.fetch_geometry_factor, 1.0)
                self.assertEqual(cell.fetch_geometry_factor_low, 1.0)
                self.assertEqual(cell.fetch_geometry_factor_high, 1.0)

    def test_communication_uses_paper_sizes_not_local_serialization(self) -> None:
        for cell in build_cells():
            expected_fetch = (cell.s + 16) * 368_600 / (1024 * 1024)
            self.assertAlmostEqual(cell.fetch_communication_mib, expected_fetch)
            expected_locate = (1 + cell.compactness) * 103_000 / (1024 * 1024)
            self.assertAlmostEqual(cell.locate_communication_mib, expected_locate)
            self.assertEqual(cell.communication_provenance, "P+D+X")

    def test_locate_communication_uses_formula_counts_and_rounding_only(self) -> None:
        for cell in build_cells():
            ell_raw = 2 * cell.s * math.sqrt(max(2 * cell.s, cell.s + 32))
            floor_count = 1 + cell.bf_levels * math.floor(ell_raw)
            ceil_count = 1 + cell.bf_levels * math.ceil(ell_raw)
            bytes_per_ciphertext = 103_000

            self.assertEqual(cell.locate_ciphertexts_floor, floor_count)
            self.assertEqual(cell.locate_ciphertexts_ceil, ceil_count)
            self.assertAlmostEqual(
                cell.locate_communication_low_mib,
                floor_count * bytes_per_ciphertext / (1024 * 1024),
            )
            self.assertAlmostEqual(
                cell.locate_communication_high_mib,
                ceil_count * bytes_per_ciphertext / (1024 * 1024),
            )
            self.assertEqual(
                cell.communication_basis,
                "published-formula-derived estimate",
            )

    def test_generated_communication_exposes_formula_counts(self) -> None:
        with TemporaryDirectory() as directory:
            output = Path(directory)
            write_generated_outputs(output)
            with (output / "09_communication_estimates.csv").open(
                newline="", encoding="utf-8"
            ) as handle:
                rows = list(csv.DictReader(handle))

            self.assertEqual(len(rows), 12)
            for row in rows:
                self.assertEqual(
                    row["communication_basis"],
                    "published-formula-derived estimate",
                )
                self.assertLessEqual(
                    int(row["locate_ciphertexts_floor"]),
                    int(row["locate_ciphertexts_ceil"]),
                )

    def test_bfv_ring_dimension_is_not_claimed(self) -> None:
        mapping = {128: 8192, 256: 16384, 512: 32768}
        for cell in build_cells():
            self.assertEqual(cell.cbpir_N, mapping[cell.n])
            self.assertIsNone(cell.bfcoie_N)

    def test_generated_table_keeps_unavailable_boundaries_unavailable(self) -> None:
        with TemporaryDirectory() as directory:
            output = Path(directory)
            write_generated_outputs(output)
            with (output / "12_table2_proposal.csv").open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual(len(rows), 12)
            for row in rows:
                self.assertEqual(row["Setup_ms"], "--")
                self.assertEqual(row["Search_ms"], "--")
                self.assertEqual(row["Online_ms"], "--")
                self.assertEqual(row["Search_comm_MiB"], "--")
                self.assertNotEqual(row["Locate_comm_MiB"], "--")
                self.assertNotEqual(row["Fetch_comm_MiB"], "--")

    def test_fairness_separates_workload_and_environment_comparability(self) -> None:
        with TemporaryDirectory() as directory:
            output = Path(directory)
            write_generated_outputs(output)
            with (output / "11_fairness_audit.csv").open(
                newline="", encoding="utf-8"
            ) as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual(len(rows), 12)
            for row in rows:
                self.assertEqual(
                    row["logical_workload_comparability"],
                    "COMPARABLE WITH ESTIMATION LIMITS",
                )
                self.assertEqual(row["execution_environment_match"], "NO")
                self.assertEqual(
                    row["computation_comparison_scope"],
                    "CROSS-PLATFORM; NOT HARDWARE-NORMALIZED",
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
