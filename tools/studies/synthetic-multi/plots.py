"""Dependency-free SVG plots for the validated Phase H CSV outputs."""

import csv
from html import escape
from pathlib import Path
import statistics
import sys


COLORS = {
    "candidates": "#4477aa",
    "selected": "#228833",
    "truncated": "#cc6677",
    "false": "#ee7733",
    "match": "#4477aa",
    "locate": "#66ccee",
    "fetch": "#228833",
}


def _label(name):
    return name.replace("_", " ")


def _save(path, body, width, height, title, subtitle):
    if path.exists():
        raise FileExistsError(path)
    svg = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="white"/>',
        '<style>text{font-family:DejaVu Sans,Arial,sans-serif;fill:#222}.title{font-size:22px;font-weight:700}.sub{font-size:13px;fill:#555}.case{font-size:12px}.value{font-size:11px}</style>',
        f'<text x="24" y="32" class="title">{escape(title)}</text>',
        f'<text x="24" y="54" class="sub">{escape(subtitle)}</text>',
        body,
        '</svg>',
    ]
    path.write_text("\n".join(svg) + "\n")
    return path


def candidate_plot(directory, cases):
    width = 1200
    row_height = 35
    height = 105 + row_height * len(cases)
    left = 260
    scale = 115
    parts = []
    legends = [("Candidates", "candidates"), ("Selected", "selected"),
               ("Truncated", "truncated"), ("False selected", "false")]
    for index, (text, key) in enumerate(legends):
        x = left + index * 180
        parts.append(f'<rect x="{x}" y="67" width="14" height="14" fill="{COLORS[key]}"/>')
        parts.append(f'<text x="{x + 20}" y="79" class="case">{text}</text>')
    for row, case in enumerate(cases):
        y = 102 + row * row_height
        parts.append(f'<text x="250" y="{y + 14}" text-anchor="end" class="case">{escape(_label(case["case"]))}</text>')
        values = [int(case["candidate_count"]), int(case["selected_count"]),
                  int(case["truncated_count"]), int(case["false_selected_count"])]
        for offset, (value, key) in enumerate(zip(values, ("candidates", "selected", "truncated", "false"))):
            bar_y = y + offset * 6
            parts.append(f'<rect x="{left}" y="{bar_y}" width="{value * scale}" height="5" fill="{COLORS[key]}"/>')
            if value:
                parts.append(f'<text x="{left + value * scale + 5}" y="{bar_y + 6}" class="value">{value}</text>')
    return _save(
        directory / "candidate_flow.svg", "\n".join(parts), width, height,
        "P0-D K=4 synthetic candidate flow",
        "Counts are per frozen case. False selected means no literal match; it can still be an exact RFPS score acceptance.",
    )


def timing_plot(directory, timings):
    grouped = {}
    for row in timings:
        grouped.setdefault(row["case"], []).append(row)
    medians = []
    for name, rows in grouped.items():
        medians.append((name, {
            stage: statistics.median(float(row[f"{stage}_ns"]) for row in rows) / 1_000_000
            for stage in ("match", "locate", "fetch")
        }))
    maximum = max(value for _, stages in medians for value in stages.values()) or 1
    width = 1200
    row_height = 42
    height = 105 + row_height * len(medians)
    left = 260
    scale = 820 / maximum
    parts = []
    for index, stage in enumerate(("match", "locate", "fetch")):
        x = left + index * 155
        parts.append(f'<rect x="{x}" y="67" width="14" height="14" fill="{COLORS[stage]}"/>')
        parts.append(f'<text x="{x + 20}" y="79" class="case">{stage.title()}</text>')
    for row, (name, stages) in enumerate(medians):
        y = 102 + row * row_height
        parts.append(f'<text x="250" y="{y + 18}" text-anchor="end" class="case">{escape(_label(name))}</text>')
        for offset, stage in enumerate(("match", "locate", "fetch")):
            value = stages[stage]
            bar_y = y + offset * 11
            parts.append(f'<rect x="{left}" y="{bar_y}" width="{value * scale:.2f}" height="9" fill="{COLORS[stage]}"/>')
            parts.append(f'<text x="{left + value * scale + 5:.2f}" y="{bar_y + 9}" class="value">{value:.3f} ms</text>')
    return _save(
        directory / "stage_timings.svg", "\n".join(parts), width, height,
        "P0-D K=4 synthetic stage timings",
        "Median of 3 measured requests per case; one warmup excluded. Local machine only. Locate is client plaintext work.",
    )


def render(directory, cases, timings):
    directory.mkdir(parents=True, exist_ok=True)
    if not cases or not timings:
        raise ValueError("plot inputs must not be empty")
    return [candidate_plot(directory, cases), timing_plot(directory, timings)]


def main(arguments):
    if len(arguments) != 2:
        raise SystemExit("usage: plots.py OUTPUT_DIR")
    directory = Path(arguments[1])
    with (directory / "cases.csv").open() as source:
        cases = list(csv.DictReader(source))
    with (directory / "timings.csv").open() as source:
        timings = list(csv.DictReader(source))
    for path in render(directory, cases, timings):
        print(path)


if __name__ == "__main__":
    main(sys.argv)
