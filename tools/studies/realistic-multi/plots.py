"""Dependency-free SVG plots for the validated realistic K=4 study."""

import csv
from html import escape
from pathlib import Path
import statistics
import sys


COLORS = ["#4477aa", "#228833", "#ee7733", "#cc6677"]


def _save(path, title, subtitle, body, width=1100, height=620):
    if path.exists():
        raise FileExistsError(path)
    svg = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="white"/>',
        '<style>text{font-family:DejaVu Sans,Arial,sans-serif;fill:#222}.title{font-size:23px;font-weight:700}.sub{font-size:13px;fill:#555}.label{font-size:13px}.value{font-size:12px}</style>',
        f'<text x="28" y="36" class="title">{escape(title)}</text>',
        f'<text x="28" y="59" class="sub">{escape(subtitle)}</text>',
        body,
        '</svg>',
    ]
    path.write_text("\n".join(svg) + "\n")
    return path


def candidate_distribution_plot(directory, distribution):
    maximum = max(int(row["requests"]) for row in distribution)
    chart_height = 430
    baseline = 540
    parts = ['<line x1="90" y1="540" x2="1030" y2="540" stroke="#444"/>']
    bar_width = 760 / len(distribution)
    for index, row in enumerate(distribution):
        size = int(row["candidate_size"])
        requests = int(row["requests"])
        height = chart_height * requests / maximum
        x = 125 + index * bar_width
        y = baseline - height
        parts.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{bar_width * .62:.1f}" height="{height:.1f}" fill="{COLORS[0]}"/>')
        parts.append(f'<text x="{x + bar_width * .31:.1f}" y="{y - 7:.1f}" text-anchor="middle" class="value">{requests}</text>')
        parts.append(f'<text x="{x + bar_width * .31:.1f}" y="563" text-anchor="middle" class="label">{size}</text>')
    parts.append('<text x="560" y="595" text-anchor="middle" class="label">Distinct native candidate RecordIds per request</text>')
    return _save(
        directory / "candidate_distribution.svg",
        "P0-D realistic K=4 candidate-set distribution",
        "643 frozen BGL requests. Labels are request counts; this bounded workload is not a deployment distribution.",
        "\n".join(parts),
    )


def stage_timing_plot(directory, attempts, timings):
    mode_by_case = {row["case_id"]: row["mode"] for row in attempts}
    groups = {
        mode: [row for row in timings if mode_by_case[row["case_id"]] == mode]
        for mode in ("fps", "rfps")
    }
    stages = ("match", "locate", "fetch")
    values = {
        (mode, stage): statistics.median(float(row[f"{stage}_ns"]) for row in rows) / 1_000_000
        for mode, rows in groups.items() if rows
        for stage in stages
    }
    maximum = max(values.values()) or 1
    parts = []
    present_modes = [mode for mode, rows in groups.items() if rows]
    for mode_index, mode in enumerate(present_modes):
        parts.append(f'<text x="170" y="{180 + mode_index * 190}" text-anchor="end" class="label">{mode.upper()}</text>')
        for stage_index, stage in enumerate(stages):
            value = values[(mode, stage)]
            y = 130 + mode_index * 190 + stage_index * 45
            width = 760 * value / maximum
            parts.append(f'<rect x="190" y="{y}" width="{width:.2f}" height="28" fill="{COLORS[stage_index]}"/>')
            parts.append(f'<text x="{200 + width:.2f}" y="{y + 20}" class="value">{stage}: {value:.3f} ms</text>')
    return _save(
        directory / "stage_timings.svg",
        "P0-D realistic K=4 stage timings",
        "Median per request by Match mode. Local wall time; Locate is clear client work; no network transport.",
        "\n".join(parts),
    )


def boundary_dedup_plot(directory, attempts):
    metrics = [
        ("Requests with boundary rejection", sum(int(row["boundary_rejected_count"]) > 0 for row in attempts)),
        ("Rejected boundary positions", sum(int(row["boundary_rejected_count"]) for row in attempts)),
        ("Requests using deduplication", sum(int(row["duplicate_count"]) > 0 for row in attempts)),
        ("Removed duplicate positions", sum(int(row["duplicate_count"]) for row in attempts)),
        ("Requests exceeding K=4", sum(int(row["candidate_count"]) > 4 for row in attempts)),
        ("Truncated candidates", sum(int(row["truncated_count"]) for row in attempts)),
    ]
    maximum = max(value for _, value in metrics) or 1
    parts = []
    for index, (label, value) in enumerate(metrics):
        y = 105 + index * 72
        width = 720 * value / maximum
        parts.append(f'<text x="265" y="{y + 22}" text-anchor="end" class="label">{escape(label)}</text>')
        parts.append(f'<rect x="285" y="{y}" width="{width:.2f}" height="30" fill="{COLORS[index % len(COLORS)]}"/>')
        parts.append(f'<text x="{297 + width:.2f}" y="{y + 22}" class="value">{value}</text>')
    return _save(
        directory / "boundary_dedup.svg",
        "P0-D realistic K=4 Locate and selection events",
        "Request counts and position/candidate counts are different units and are shown on separate bars.",
        "\n".join(parts),
    )


def rfps_plot(directory, attempts):
    rows = [row for row in attempts if row["mode"] == "rfps"]
    groups = {}
    for row in rows:
        key = (row["weight_policy"], float(row["threshold"]))
        group = groups.setdefault(key, {"selected": 0, "false": 0, "requests": 0})
        group["selected"] += int(row["selected_count"])
        group["false"] += int(row["false_selected_count"])
        group["requests"] += 1
    maximum = max(group["selected"] for group in groups.values()) or 1
    parts = []
    for index, (key, group) in enumerate(sorted(groups.items())):
        policy, threshold = key
        y = 100 + index * 72
        selected_width = 700 * group["selected"] / maximum
        false_width = 700 * group["false"] / maximum
        label = f"{policy}, threshold {threshold:g}"
        parts.append(f'<text x="280" y="{y + 18}" text-anchor="end" class="label">{escape(label)}</text>')
        parts.append(f'<rect x="300" y="{y}" width="{selected_width:.2f}" height="22" fill="{COLORS[1]}"/>')
        parts.append(f'<rect x="300" y="{y + 26}" width="{false_width:.2f}" height="14" fill="{COLORS[2]}"/>')
        parts.append(f'<text x="{310 + selected_width:.2f}" y="{y + 17}" class="value">selected {group["selected"]}</text>')
        parts.append(f'<text x="{310 + false_width:.2f}" y="{y + 39}" class="value">false selected {group["false"]}</text>')
    return _save(
        directory / "rfps_propagation.svg",
        "P0-D realistic K=4 RFPS selected-record propagation",
        "Green is all selected records; orange is selected RecordIds without a literal match. Frozen workload only.",
        "\n".join(parts),
    )


def render(directory, attempts, distribution, timings):
    directory.mkdir(parents=True, exist_ok=True)
    if not attempts or not distribution or not timings:
        raise ValueError("plot inputs must not be empty")
    return [
        candidate_distribution_plot(directory, distribution),
        stage_timing_plot(directory, attempts, timings),
        boundary_dedup_plot(directory, attempts),
        rfps_plot(directory, attempts),
    ]


def main(arguments):
    if len(arguments) != 2:
        raise SystemExit("usage: plots.py OUTPUT_DIR")
    directory = Path(arguments[1])
    with (directory / "attempts.csv").open() as source:
        attempts = list(csv.DictReader(source))
    with (directory / "candidate_distribution.csv").open() as source:
        distribution = list(csv.DictReader(source))
    with (directory / "timings.csv").open() as source:
        timings = list(csv.DictReader(source))
    for path in render(directory, attempts, distribution, timings):
        print(path)


if __name__ == "__main__":
    main(sys.argv)
