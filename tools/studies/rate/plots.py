"""Data-only plots from validated CSV summaries; no fitted or invented rates."""
import csv
import os
from pathlib import Path
import sys

# Keep font/cache writes inside P0-D even when invoked from repository root.
os.environ.setdefault("MPLCONFIGDIR", str(Path(__file__).resolve().parents[1] / "target" / "mplconfig"))
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from generate import FAMILIES, LENGTHS, POLICIES, THRESHOLDS

POLICY_LABELS = {"uniform_3_32": "Random weights 3–32", "ones_diagnostic": "All-ones weights — diagnostic"}
FAMILY_LABELS = {"full_bytes": "Independent full bytes", "small_alphabet": "Independent alphabet 0–3",
                 "one_change": "One coordinate +1", "balanced": "Two coordinates +1 / −1"}
OUTCOMES = ("correct_record", "false_record", "missed_true", "correct_empty", "execution_error")
THRESHOLD_MARKERS = {0.5: "o", 1.0: "s", 4.0: "^"}


def save(fig, directory, name):
    paths = []
    for suffix in ("png", "svg"):
        path = directory / f"{name}.{suffix}"
        if path.exists():
            raise FileExistsError(path)
        fig.savefig(path, dpi=160, facecolor="white")
        paths.append(path)
    plt.close(fig)
    return paths


def save_csv(directory, name, rows):
    path = directory / name
    if path.exists():
        raise FileExistsError(path)
    if not rows:
        raise ValueError(f"no plot rows for {name}")
    with path.open("x", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    return path


def heatmaps(directory, rates):
    paths = []
    lookup = {(r["source"], r["policy"], r["family"], int(r["length"]), float(r["threshold"])): r for r in rates}
    for source in ("exact", "native"):
        for policy in POLICIES:
            fig, axes = plt.subplots(2, 2, figsize=(12, 9), layout="constrained")
            for ax, family in zip(axes.flat, FAMILIES):
                values = np.full((len(LENGTHS), len(THRESHOLDS)), np.nan)
                counts = {}
                for i, length in enumerate(LENGTHS):
                    for j, threshold in enumerate(THRESHOLDS):
                        row = lookup.get((source, policy, family, length, threshold))
                        if row and int(row["nonmatch_windows"]):
                            numerator, denominator = int(row["false_matches"]), int(row["nonmatch_windows"])
                            values[i, j] = 100*numerator/denominator
                            counts[i, j] = numerator, denominator
                cmap = plt.get_cmap("cividis").copy()
                cmap.set_bad("#eeeeee")
                picture = ax.imshow(values, vmin=0, vmax=100, cmap=cmap, aspect="auto")
                for i in range(len(LENGTHS)):
                    for j in range(len(THRESHOLDS)):
                        text = "N/A"
                        if (i, j) in counts:
                            a, b = counts[i, j]
                            text = f"{values[i,j]:.2f}%\n{a}/{b}"
                        ax.text(j, i, text, ha="center", va="center", fontsize=10,
                                color="white" if np.isfinite(values[i,j]) and values[i,j] < 45 else "black")
                ax.set(xticks=range(len(THRESHOLDS)), xticklabels=THRESHOLDS,
                       yticks=range(len(LENGTHS)), yticklabels=LENGTHS,
                       xlabel="Acceptance threshold τ", ylabel="Pattern length (bytes)", title=FAMILY_LABELS[family])
            fig.colorbar(picture, ax=axes, label="Accepted nonmatching windows (%)", shrink=0.8)
            label = "Exact integer score" if source == "exact" else "Native CKKS score"
            fig.suptitle(f"P0-D RFPS false matches | {label}\n{POLICY_LABELS[policy]}\n"
                         "Each cell: observed percent and false matches / nonmatching windows. Zero is not a guarantee.", fontsize=13)
            paths += save(fig, directory, f"heatmap_{source}_{policy}")
    return paths


def cause_plot(directory, causes):
    fig, axes = plt.subplots(2, 3, figsize=(16, 8), layout="constrained")
    keys = ("intrinsic_cancellation", "threshold_only", "ckks_false_positive", "ckks_false_negative")
    labels = ("Cancellation\n/nonmatches", "Threshold-only\n/nonmatches", "CKKS false accept\n/all windows", "CKKS false reject\n/all windows")
    lookup = {(r["source"], r["policy"], float(r["threshold"])): r for r in causes}
    for i, policy in enumerate(POLICIES):
        for j, threshold in enumerate(THRESHOLDS):
            ax = axes[i, j]
            for source, offset, color in (("exact", -0.19, "#2878b5"), ("native", 0.19, "#e68613")):
                row = lookup.get((source, policy, threshold))
                for k, key in enumerate(keys):
                    if source == "exact" and k >= 2:
                        continue
                    numerator = int(row[key]) if row else 0
                    denominator = int(row["nonmatch_windows" if k < 2 else "windows"]) if row else 0
                    if denominator:
                        value = 100*numerator/denominator
                        ax.bar(k+offset, value, 0.36, color=color, label=source if k == 0 else None)
                        ax.text(k+offset, value+1.2, f"{numerator}/{denominator}", ha="center", va="bottom", rotation=90, fontsize=8)
            ax.set(xticks=range(4), xticklabels=labels, ylim=(0, 120), ylabel="Observed percent",
                   title=f"{POLICY_LABELS[policy]} | τ={threshold:g}")
            ax.tick_params(axis="x", labelsize=8)
            ax.grid(axis="y", alpha=0.2)
    axes[0, 0].legend(loc="upper right")
    fig.suptitle("P0-D RFPS effects — grouped, not additive\n"
                 "Fixed synthetic mixture of lengths/families, not a deployment rate. Exact scoring has no CKKS error bars.\n"
                 "Native 'all windows' includes true controls and planted matches; nonmatch denominators exclude them.", fontsize=12)
    return save(fig, directory, "cause_comparison")


def score_plot(directory, windows):
    fig, axes = plt.subplots(2, 2, figsize=(13, 10), layout="constrained")
    for column, policy in enumerate(POLICIES):
        rows = [r for r in windows if r["policy"] == policy]
        for threshold in THRESHOLDS:
            for same, color, label in ((False, "#cc6b17", "Nonmatch"), (True, "#2878b5", "Literal match")):
                group = [r for r in rows if r["semantic_match"] in (same, str(same))
                         and float(r["threshold"]) == threshold]
                x = [float(r["exact_score"]) for r in group]
                y = [float(r["approximate_score"]) for r in group]
                marker = THRESHOLD_MARKERS[threshold]
                axes[0, column].scatter(x, y, s=12, alpha=0.35, color=color, marker=marker,
                                        label=f"{label}, τ={threshold:g}")
                near = [(a, b) for a, b in zip(x, y) if abs(a) <= 5 and abs(b) <= 5]
                axes[1, column].scatter([a for a, b in near], [b for a, b in near],
                                        s=22, alpha=0.45, color=color, marker=marker,
                                        label=f"{label}, τ={threshold:g}")
        full = axes[0, column]
        full.set(xlabel="Exact integer score", ylabel="Decrypted CKKS score",
                 title=f"{POLICY_LABELS[policy]} | {len(rows)} windows")
        limits = full.get_xlim()
        full.plot(limits, limits, color="gray", linestyle="--", linewidth=0.8)
        full.legend(fontsize=7, ncol=2)
        zoom = axes[1, column]
        styles = {0.5: ":", 1.0: "--", 4.0: "-."}
        for threshold in THRESHOLDS:
            for sign in (-1, 1):
                zoom.axvline(sign*threshold, color="gray", linestyle=styles[threshold], linewidth=0.8)
                zoom.axhline(sign*threshold, color="gray", linestyle=styles[threshold], linewidth=0.8)
        zoom.plot((-5, 5), (-5, 5), color="black", linestyle="--", linewidth=0.7)
        zoom.set(xlim=(-5, 5), ylim=(-5, 5), xlabel="Exact integer score",
                 ylabel="Decrypted CKKS score",
                 title="Signed zoom | ±0.5 dotted; ±1 dashed; ±4 dash-dot")
        zoom.grid(alpha=0.2)
        zoom.text(0.03, 0.97, "Marker: ○ τ=0.5, □ τ=1, △ τ=4\n"
                  "A request accepts when both score coordinates lie within its own ±τ.",
                  transform=zoom.transAxes, va="top", fontsize=8)
    fig.suptitle("P0-D RFPS exact and decrypted scores\n"
                 "All three thresholds shown; repeated inputs use fresh HE randomness.\n"
                 "Explicit boundaries: ±0.5, ±1, ±4. Points outside zoom remain in CSV and top panels.", fontsize=13)
    return save(fig, directory, "score_boundaries")


def outcome_plot(directory, outcomes):
    fig, axes = plt.subplots(1, 2, figsize=(15, 7), layout="constrained")
    layouts = ("no_match", "competing_true", "true_only")
    labels = ("No true record", "Competing true", "True only")
    colors = ("#228855", "#e68613", "#8855aa", "#66aacc", "#cc3344")
    lookup = {(r["policy"], float(r["threshold"]), r["layout"]): r for r in outcomes}
    for ax, policy in zip(axes, POLICIES):
        for i, layout in enumerate(layouts):
            for j, threshold in enumerate(THRESHOLDS):
                row = lookup.get((policy, threshold, layout))
                position, bottom = i*3+j, 0
                denominator = int(row["attempts"]) if row else 0
                for key, color in zip(OUTCOMES, colors):
                    numerator = int(row[key]) if row else 0
                    value = 100*numerator/denominator if denominator else 0
                    ax.bar(position, value, 0.8, bottom=bottom, color=color,
                           label=key.replace("_", " ") if position == 0 else None)
                    if value >= 7:
                        ax.text(position, bottom+value/2, str(numerator), ha="center", va="center", fontsize=9)
                    bottom += value
                ax.text(position, 102, f"n={denominator}" if denominator else "N/A", ha="center", fontsize=8)
        ax.set(xticks=range(9), xticklabels=[f"{label}\nτ={t:g}" for label in labels for t in THRESHOLDS],
               ylim=(0, 112), ylabel="Attempted requests (%)", title=POLICY_LABELS[policy])
        ax.tick_params(axis="x", rotation=35, labelsize=8)
    handles, labels = axes[0].get_legend_handles_labels()
    fig.legend(handles, labels, loc="outside lower center", ncol=5)
    fig.suptitle("P0-D K=1 outcomes — mutually exclusive categories\n"
                 "Bars show request percentages; segment labels are counts. Lower false ID competes with planted true ID.\n"
                 "No-match and competing layouts are deliberate synthetic stress tests, not observed database prevalence.", fontsize=12)
    return save(fig, directory, "k1_outcomes")


def render(directory, rates, causes, windows, outcomes):
    directory.mkdir(parents=True, exist_ok=True)
    plt.rcParams.update({"font.size": 10, "svg.fonttype": "none"})
    csv_paths = [
        save_csv(directory, "heatmap_rates.csv", rates),
        save_csv(directory, "cause_comparison.csv", causes),
        save_csv(directory, "score_boundaries.csv", windows),
        save_csv(directory, "k1_outcomes.csv", outcomes),
    ]
    return (csv_paths+heatmaps(directory, rates)+cause_plot(directory, causes)
            +score_plot(directory, windows)+outcome_plot(directory, outcomes))


def main(directory):
    def read(name):
        with (directory / name).open() as source:
            return list(csv.DictReader(source))
    paths = render(directory / "plots_final", read("rates.csv"), read("cause_counts.csv"),
                   read("native_windows.csv"), read("outcome_counts.csv"))
    for path in paths:
        print(path)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
