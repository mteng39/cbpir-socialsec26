#!/usr/bin/env python3
"""Recompute final camera-ready aggregate comparison from retained CSV files."""

from __future__ import annotations

import csv
import json
from decimal import Decimal, ROUND_HALF_UP, getcontext
from pathlib import Path


getcontext().prec = 50
ROOT = Path(__file__).resolve().parents[2]
THREE_DECIMALS = Decimal("0.001")


def rows(relative: str) -> list[dict[str, str]]:
    with (ROOT / relative).open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def printed_milliseconds(value: str) -> Decimal:
    return Decimal(value).quantize(THREE_DECIMALS, rounding=ROUND_HALF_UP)


def compute() -> dict[str, Decimal | int]:
    cbpir_latency = rows("evaluation/formal-benchmark/results/07_online_summary.csv")
    cbpir_communication = rows(
        "evaluation/formal-benchmark/results/08_communication.csv"
    )
    bfcoie_latency = rows("evaluation/bf-coie-baseline/08_latency_estimates.csv")
    bfcoie_communication = rows(
        "evaluation/bf-coie-baseline/09_communication_estimates.csv"
    )

    if len(cbpir_latency) != 24 or len(cbpir_communication) != 24:
        raise ValueError("expected 24 CBPIR configurations")
    if len(bfcoie_latency) != 12 or len(bfcoie_communication) != 12:
        raise ValueError("expected 12 BF-COIE configurations")

    cbpir_latency_ms = sum(
        (
            printed_milliseconds(row["match_median_ms"])
            + printed_milliseconds(row["locate_median_ms"])
            + printed_milliseconds(row["fetch_median_ms"])
            for row in cbpir_latency
        ),
        Decimal(0),
    )
    bfcoie_latency_ms = Decimal(2) * sum(
        (Decimal(row["postmatch_ms"]) for row in bfcoie_latency), Decimal(0)
    )
    cbpir_communication_mib = sum(
        (Decimal(row["total_mib"]) for row in cbpir_communication), Decimal(0)
    )
    bfcoie_communication_mib = Decimal(2) * sum(
        (
            Decimal(row["postmatch_communication_mib"])
            for row in bfcoie_communication
        ),
        Decimal(0),
    )

    return {
        "configuration_count": len(cbpir_latency),
        "cbpir_latency_ms": cbpir_latency_ms,
        "bfcoie_latency_ms": bfcoie_latency_ms,
        "latency_ratio": bfcoie_latency_ms / cbpir_latency_ms,
        "cbpir_communication_mib": cbpir_communication_mib,
        "bfcoie_communication_mib": bfcoie_communication_mib,
        "communication_ratio": bfcoie_communication_mib
        / cbpir_communication_mib,
    }


def main() -> None:
    result = compute()
    print(json.dumps({key: str(value) for key, value in result.items()}, indent=2))


if __name__ == "__main__":
    main()
