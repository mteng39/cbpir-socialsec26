"""Human-readable plots for the bounded BGL K=1 study."""
import csv
from collections import defaultdict
import math
from pathlib import Path
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


COLORS = ("#35618f", "#d17c28", "#559c74", "#8d65a5")


def _read(path):
    with Path(path).open(newline="") as source:
        return list(csv.DictReader(source))


def _write(path, rows, fields=None):
    if not rows:
        raise ValueError(f"empty plot table: {path}")
    with path.open("x", newline="") as destination:
        writer = csv.DictWriter(
            destination, fieldnames=fields or list(rows[0]), lineterminator="\n"
        )
        writer.writeheader()
        writer.writerows(rows)


def _save(figure, output, stem):
    figure.savefig(output / f"{stem}.png", dpi=220, bbox_inches="tight")
    svg_path = output / f"{stem}.svg"
    figure.savefig(svg_path, bbox_inches="tight")
    svg_path.write_text(
        "\n".join(line.rstrip() for line in svg_path.read_text().splitlines()) + "\n"
    )
    plt.close(figure)


def _candidate_funnel(attempts, output):
    framed = sum(
        int(row["approximate_accept_valid"]) + int(row["approximate_accept_boundary"])
        for row in attempts
    )
    valid = sum(int(row["approximate_accept_valid"]) for row in attempts)
    candidates = sum(int(row["locate_candidate_records"]) for row in attempts)
    selected = sum(int(row["selected_real"]) for row in attempts)
    rows = [
        {"stage": "CKKS accepted offsets", "count": framed, "unit": "offset-decision"},
        {"stage": "After boundary rejection", "count": valid, "unit": "valid offset-decision"},
        {"stage": "After RecordId dedup", "count": candidates, "unit": "candidate-record/request"},
        {"stage": "K=1 real selections", "count": selected, "unit": "request"},
    ]
    _write(output / "candidate_funnel.csv", rows)
    figure, axis = plt.subplots(figsize=(9, 4.8))
    bars = axis.barh([row["stage"] for row in rows][::-1], [row["count"] for row in rows][::-1], color=COLORS)
    axis.bar_label(bars, fmt="%.0f", padding=4)
    axis.set_xlabel("Count (units change by stage; see source CSV)")
    axis.set_title("P0-D K=1 candidate funnel — all 643 requests")
    axis.grid(axis="x", alpha=0.25)
    _save(figure, output, "candidate_funnel")


def _heatmap(attempts, output):
    grouped = defaultdict(lambda: [0, 0])
    for row in attempts:
        if row["mode"] != "rfps":
            continue
        key = (row["weight_policy"], row["pattern_kind"], float(row["threshold"]))
        grouped[key][0] += int(row["exact_intrinsic_cancellation"]) + int(row["exact_threshold_only"])
        grouped[key][1] += int(row["nonliteral_valid_windows"])
    table = []
    for (policy, kind, threshold), (numerator, denominator) in sorted(grouped.items()):
        table.append(
            {
                "weight_policy": policy,
                "pattern_kind": kind,
                "threshold": threshold,
                "exact_semantic_false_accepts": numerator,
                "nonliteral_valid_window_decisions": denominator,
                "percent": 100 * numerator / denominator if denominator else 0,
            }
        )
    _write(output / "rfps_false_acceptance_heatmap.csv", table)
    policies = ("uniform_3_32", "ones_diagnostic")
    figure, axes = plt.subplots(1, 2, figsize=(15, 6), constrained_layout=True)
    for axis, policy in zip(axes, policies):
        subset = [row for row in table if row["weight_policy"] == policy]
        kinds = sorted({row["pattern_kind"] for row in subset})
        thresholds = (0.5, 1.0, 4.0)
        values = np.full((len(kinds), len(thresholds)), np.nan)
        labels = [["—" for _ in thresholds] for _ in kinds]
        for row in subset:
            y = kinds.index(row["pattern_kind"])
            x = thresholds.index(float(row["threshold"]))
            values[y, x] = float(row["percent"])
            labels[y][x] = (
                f'{int(row["exact_semantic_false_accepts"])}/'
                f'{int(row["nonliteral_valid_window_decisions"])}\n{row["percent"]:.2f}%'
            )
        image = axis.imshow(np.ma.masked_invalid(values), aspect="auto", cmap="magma", vmin=0)
        axis.set_xticks(range(len(thresholds)), [str(value) for value in thresholds])
        axis.set_yticks(range(len(kinds)), [kind.replace("_", " ") for kind in kinds])
        axis.set_xlabel("Inclusive threshold τ")
        axis.set_title(policy.replace("_", " "))
        for y in range(len(kinds)):
            for x in range(len(thresholds)):
                axis.text(x, y, labels[y][x], ha="center", va="center", color="white", fontsize=7)
        figure.colorbar(image, ax=axis, label="Exact semantic false accepts (%)")
    figure.suptitle("Exact RFPS false acceptance by pattern class\n(numbers are accepted/nonliteral valid window-decisions)")
    _save(figure, output, "rfps_false_acceptance_heatmap")


def _multiplicity(multiplicity, output):
    fields = (
        "pattern_id", "pattern_kind", "length", "literal_occurrences",
        "distinct_record_ids", "within_record_duplicate_offsets", "record_occurrence_counts",
    )
    _write(output / "pattern_multiplicity.csv", multiplicity, fields)
    labels = [row["pattern_id"] for row in multiplicity]
    occurrences = [int(row["literal_occurrences"]) for row in multiplicity]
    records = [int(row["distinct_record_ids"]) for row in multiplicity]
    duplicates = [int(row["within_record_duplicate_offsets"]) for row in multiplicity]
    x = np.arange(len(labels))
    width = 0.25
    figure, axis = plt.subplots(figsize=(14, 5.5))
    axis.bar(x - width, occurrences, width, label="literal offsets", color=COLORS[0])
    axis.bar(x, records, width, label="distinct RecordIds", color=COLORS[1])
    axis.bar(x + width, duplicates, width, label="extra offsets within record", color=COLORS[2])
    axis.set_xticks(x, labels, rotation=42, ha="right")
    axis.set_ylabel("Count per frozen natural pattern")
    axis.set_title("Literal occurrence and RecordId multiplicity")
    axis.legend()
    axis.grid(axis="y", alpha=0.25)
    _save(figure, output, "pattern_multiplicity")


def _runtime(attempts, output):
    mapping = (
        ("Match", "match_ns"), ("Locate", "locate_ns"),
        ("Fetch", "fetch_ns"), ("End-to-end", "request_ns"),
    )
    rows = []
    values = []
    for label, field in mapping:
        samples = [int(row[field]) / 1_000_000 for row in attempts]
        values.append(samples)
        rows.extend(
            {"case_id": row["case_id"], "stage": label, "milliseconds": sample}
            for row, sample in zip(attempts, samples)
        )
    _write(output / "stage_runtime.csv", rows)
    figure, axis = plt.subplots(figsize=(9, 5.5))
    boxes = axis.boxplot(values, tick_labels=[label for label, _ in mapping], patch_artist=True, showfliers=False)
    for patch, color in zip(boxes["boxes"], COLORS):
        patch.set_facecolor(color)
        patch.set_alpha(0.8)
    axis.set_yscale("log")
    axis.set_ylabel("Milliseconds, log scale")
    axis.set_title("Measured request-stage distributions — 643 requests")
    axis.text(
        0.01, -0.18,
        "Each box uses raw per-request samples. Component medians are not summed.",
        transform=axis.transAxes,
    )
    axis.grid(axis="y", which="both", alpha=0.25)
    _save(figure, output, "stage_runtime")


def render(attempts_path, multiplicity_path, output_directory):
    attempts = [row for row in _read(attempts_path) if not row.get("error")]
    multiplicity = _read(multiplicity_path)
    if not attempts or not multiplicity:
        raise ValueError("plots require successful attempts and multiplicity rows")
    output = Path(output_directory)
    output.mkdir(parents=True, exist_ok=False)
    _candidate_funnel(attempts, output)
    _heatmap(attempts, output)
    _multiplicity(multiplicity, output)
    _runtime(attempts, output)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: plots.py ATTEMPTS_CSV MULTIPLICITY_CSV NEW_PLOTS_DIRECTORY")
    render(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]))
