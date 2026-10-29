"""Exact finite enumeration for fixed RFPS mismatch structures."""
from collections import defaultdict
import csv
from fractions import Fraction
import itertools
import json
import math
from pathlib import Path
import sys


def acceptance_fraction(differences, weight_support, threshold):
    differences = tuple(differences)
    weights = tuple(weight_support)
    if not differences:
        raise ValueError("at least one mismatch difference is required")
    if not weights:
        raise ValueError("weight support cannot be empty")
    if not math.isfinite(threshold) or threshold < 0:
        raise ValueError("threshold must be finite and nonnegative")
    accepted = sum(
        abs(sum(delta * weight for delta, weight in zip(differences, draw))) <= threshold
        for draw in itertools.product(weights, repeat=len(differences))
    )
    return Fraction(accepted, len(weights) ** len(differences))


def build_report(rows):
    models = {
        "balanced": (1, -1),
        "one_change": (1,),
    }
    supports = {
        "uniform_3_32": tuple(range(3, 33)),
        "ones_diagnostic": (1,),
    }
    theory = {}
    for family, differences in models.items():
        for policy, support in supports.items():
            for threshold in (0.5, 1, 4):
                probability = acceptance_fraction(differences, support, threshold)
                total_assignments = len(support) ** len(differences)
                accepted_assignments = (
                    probability.numerator * total_assignments // probability.denominator
                )
                key = f"{family}/{policy}/{threshold:g}"
                theory[key] = {
                    "differences": list(differences),
                    "weight_support": list(support),
                    "accepted_assignments": accepted_assignments,
                    "total_assignments": total_assignments,
                    "fraction": str(probability),
                    "percent": 100 * float(probability),
                }

    counts = defaultdict(lambda: [0, 0])
    for row in rows:
        if row["source"] != "exact" or row["family"] not in models:
            continue
        key = f'{row["family"]}/{row["policy"]}/{float(row["threshold"]):g}'
        counts[key][0] += int(row["false_matches"])
        counts[key][1] += int(row["nonmatch_windows"])
    observed = {
        key: {
            "false_matches": numerator,
            "nonmatch_windows": denominator,
            "percent": 100 * numerator / denominator,
        }
        for key, (numerator, denominator) in sorted(counts.items())
    }
    return {
        "schema": "p0d-rfps-analytical-check-v1",
        "classification": "exact finite enumeration and retained-experiment cross-check",
        "theory": theory,
        "observed": observed,
        "limits": [
            "Theory is conditional on the displayed fixed mismatch vector and weight support",
            "Observed rows are finite seeded samples, not exact probability identities",
            "This report contains no CKKS, modular-arithmetic, or security estimate",
        ],
    }


def main(path):
    with path.open(newline="") as source:
        report = build_report(csv.DictReader(source))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main(Path(sys.argv[1]))
