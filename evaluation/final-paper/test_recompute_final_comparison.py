import importlib.util
import unittest
from decimal import Decimal
from pathlib import Path


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "recompute_final_comparison", HERE / "recompute_final_comparison.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FinalComparisonTests(unittest.TestCase):
    def test_revised_aggregate_ratios_match_final_paper(self):
        result = MODULE.compute()

        self.assertEqual(result["configuration_count"], 24)
        self.assertEqual(result["cbpir_latency_ms"], Decimal("1204.992"))
        self.assertEqual(
            result["bfcoie_latency_ms"], Decimal("37976.0364937486336")
        )
        self.assertEqual(
            result["latency_ratio"],
            Decimal("37976.0364937486336") / Decimal("1204.992"),
        )
        self.assertEqual(
            result["communication_ratio"],
            Decimal("1248.766708374023444") / Decimal("84.1147384643554680"),
        )


if __name__ == "__main__":
    unittest.main()
