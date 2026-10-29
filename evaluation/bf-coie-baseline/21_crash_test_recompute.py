#!/usr/bin/env python3
"""Independent reconstruction for the CBPIR/BF-COIE reviewer crash test.

This script deliberately does not import estimate.py or consume its generated
BF-COIE CSVs.  It reads the manuscript only as the target under review,
reconstructs CBPIR from frozen raw evidence, and reconstructs BF-COIE from the
paper-extracted Figure 4 pixel heights and published protocol formulas.
"""

from __future__ import annotations

import csv
import math
import re
import statistics
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
EST = ROOT / "experiments/cbpir-scaling/bf-coie-paper-estimates"
RAW = ROOT / "experiments/cbpir-scaling/formal-seven-procedure-benchmark"
TEX = ROOT / "experiments/cbpir-scaling/cbpir_main.tex"
OUT = EST / "21_table_cell_reproduction.csv"

N_TO_N = {8192: 128, 16384: 256, 32768: 512}
N_TO_RING = {value: key for key, value in N_TO_N.items()}
TARGET_K = (1, 4, 8, 16)
MODES = ("FPS", "RFPS")
FP = 16
ETA = 2
CT_BYTES = 103_000
PIR_BYTES = 368_600
MIB = 1024 * 1024

# Independently read from Choi et al. Figure 4's embedded 2000x1400 image.
# Scale: 7.2 vertical pixels/second.  These are digitized, not textual values.
PAPER_S = (8, 16, 32, 64, 128)
SERVER_SECONDS = tuple(px / 7.2 for px in (105, 94, 84, 72, 60))
CLIENT_SECONDS = tuple(px / 7.2 for px in (2, 4, 8, 18, 33))
RETRIEVAL_SECONDS = tuple(px / 7.2 for px in (9, 14, 23, 42, 80))


def rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def truth(value: str) -> bool:
    return value.lower() == "true"


def fit(xs: list[float], ys: list[float], intercept: bool) -> tuple[float, float, float]:
    if intercept:
        xbar = statistics.mean(xs)
        ybar = statistics.mean(ys)
        slope = sum((x - xbar) * (y - ybar) for x, y in zip(xs, ys)) / sum(
            (x - xbar) ** 2 for x in xs
        )
        offset = ybar - slope * xbar
    else:
        slope = sum(x * y for x, y in zip(xs, ys)) / sum(x * x for x in xs)
        offset = 0.0
    residuals = [slope * x + offset - y for x, y in zip(xs, ys)]
    return slope, offset, math.sqrt(statistics.mean(r * r for r in residuals))


def paper_structure(s: int, hierarchy: str) -> tuple[int, int]:
    raw_t = math.log2(10_000 / (2 * s))
    t = math.floor(raw_t) if hierarchy == "floor" else math.ceil(raw_t)
    levels = t + 1
    ell = math.ceil(ETA * s * math.sqrt(max(2 * s, s + 2 * FP)))
    return ETA * 10_000 * levels, levels * ell


def target_structure(n: int, s: int, *, fp: int = FP) -> dict[str, float | int]:
    ratio = n // (2 * s)
    assert ratio * 2 * s == n and ratio > 0 and ratio & (ratio - 1) == 0
    t = ratio.bit_length() - 1
    levels = t + 1
    m = max(2 * s, s + 2 * fp)
    ell_raw = ETA * s * math.sqrt(m)
    ell = math.ceil(ell_raw)
    return {
        "t": t,
        "levels": levels,
        "m": m,
        "ell_raw": ell_raw,
        "ell": ell,
        "compactness": levels * ell,
        "attempts": ETA * n * levels,
        "pir_requests": s + fp,
    }


def bf_models() -> dict[str, float]:
    attempts_floor = [paper_structure(s, "floor")[0] for s in PAPER_S]
    compact_floor = [paper_structure(s, "floor")[1] for s in PAPER_S]
    attempts_ceil = [paper_structure(s, "ceil")[0] for s in PAPER_S]
    compact_ceil = [paper_structure(s, "ceil")[1] for s in PAPER_S]
    requests = [s + FP for s in PAPER_S]
    sf, _, server_floor_rmse = fit(attempts_floor, list(SERVER_SECONDS), False)
    cf, _, client_floor_rmse = fit(compact_floor, list(CLIENT_SECONDS), False)
    sc, _, server_ceil_rmse = fit(attempts_ceil, list(SERVER_SECONDS), False)
    cc, _, client_ceil_rmse = fit(compact_ceil, list(CLIENT_SECONDS), False)
    fo, _, fetch_origin_rmse = fit(requests, list(RETRIEVAL_SECONDS), False)
    fi, fb, fetch_intercept_rmse = fit(requests, list(RETRIEVAL_SECONDS), True)
    return {
        "server_floor_slope": sf,
        "client_floor_slope": cf,
        "server_ceil_slope": sc,
        "client_ceil_slope": cc,
        "server_floor_rmse": server_floor_rmse,
        "client_floor_rmse": client_floor_rmse,
        "server_ceil_rmse": server_ceil_rmse,
        "client_ceil_rmse": client_ceil_rmse,
        "fetch_origin_slope": fo,
        "fetch_intercept_slope": fi,
        "fetch_intercept": fb,
        "fetch_origin_rmse": fetch_origin_rmse,
        "fetch_intercept_rmse": fetch_intercept_rmse,
    }


def bf_values(n: int, k: int) -> dict[str, float | str]:
    model = bf_models()
    structure = target_structure(n, k)
    locate_s = (
        model["server_floor_slope"] * structure["attempts"]
        + model["client_floor_slope"] * structure["compactness"]
    )
    locate_alt_s = (
        model["server_ceil_slope"] * structure["attempts"]
        + model["client_ceil_slope"] * structure["compactness"]
    )
    requests = int(structure["pir_requests"])
    fetch_origin_s = model["fetch_origin_slope"] * requests
    fetch_intercept_s = max(
        0.0, model["fetch_intercept_slope"] * requests + model["fetch_intercept"]
    )
    if k in PAPER_S:
        fetch_base_s = RETRIEVAL_SECONDS[PAPER_S.index(k)]
    else:
        fetch_base_s = fetch_intercept_s
    geometry = 15 / 14 if n == 512 else 1.0
    fetch_s = fetch_base_s * geometry
    ct_count_floor = 1 + int(structure["levels"]) * math.floor(float(structure["ell_raw"]))
    ct_count_ceil = 1 + int(structure["levels"]) * math.ceil(float(structure["ell_raw"]))
    locate_comm = ct_count_ceil * CT_BYTES / MIB
    fetch_comm = requests * PIR_BYTES / MIB
    return {
        "Setup": "--",
        "Search": "--",
        "Locate": locate_s * 1000,
        "Fetch": fetch_s * 1000,
        "Online": (locate_s + fetch_s) * 1000,
        "Search_comm": "--",
        "Locate_comm": locate_comm,
        "Fetch_comm": fetch_comm,
        "Total_comm": locate_comm + fetch_comm,
        "Search_correct": "G",
        "Fetch_correct": "A",
        "k": str(k),
        "locate_low_ms": min(locate_s, locate_alt_s) * 1000,
        "locate_high_ms": max(locate_s, locate_alt_s) * 1000,
        "fetch_origin_ms": fetch_origin_s * geometry * 1000,
        "fetch_intercept_ms": fetch_intercept_s * geometry * 1000,
        "fetch_without_geometry_ms": fetch_base_s * 1000,
        "locate_comm_floor_mib": ct_count_floor * CT_BYTES / MIB,
        "pir_requests": requests,
    }


def cbpir_values() -> dict[tuple[int, int, str], dict[str, float | str]]:
    setup_groups: dict[tuple[int, str], list[float]] = defaultdict(list)
    for row in rows(RAW / "03_setup_raw.csv"):
        setup_groups[(int(row["ring_dimension"]), row["mode"].upper())].append(
            int(row["duration_ns"]) / 1e6
        )

    procedure_groups: dict[tuple[int, int, str, int], dict[str, float]] = defaultdict(dict)
    realized: dict[tuple[int, int, str], list[int]] = defaultdict(list)
    for row in rows(RAW / "05_procedure_raw.csv"):
        procedure = row["procedure"]
        if procedure in ("OnlineSum", "OnlineOuter"):
            continue
        key = (int(row["N"]), int(row["K"]), row["mode"].upper(), int(row["run_id"]))
        procedure_groups[key][procedure] = int(row["duration_ns"]) / 1e6
    for (ring, k, mode, _run), values in procedure_groups.items():
        assert set(values) == {
            "MatchQuery", "MatchResponse", "Locate", "FetchQuery", "FetchResponse", "Recover"
        }

    # realized_k occurs once per procedure, so take it from one named procedure.
    seen_runs: set[tuple[int, int, str, int]] = set()
    for row in rows(RAW / "05_procedure_raw.csv"):
        if row["procedure"] != "Locate":
            continue
        run_key = (int(row["N"]), int(row["K"]), row["mode"].upper(), int(row["run_id"]))
        assert run_key not in seen_runs
        seen_runs.add(run_key)
        realized[run_key[:3]].append(int(row["realized_k"]))

    comm: dict[tuple[int, int, str], dict[str, float]] = {}
    for row in rows(RAW / "08_communication.csv"):
        key = (int(row["N"]), int(row["K"]), row["mode"].upper())
        search = (int(row["search_query_bytes"]) + int(row["search_response_bytes"])) / MIB
        fetch = (int(row["fetch_query_bytes"]) + int(row["fetch_response_bytes"])) / MIB
        total = int(row["total_online_bytes"]) / MIB
        assert math.isclose(total, search + fetch)
        comm[key] = {"Search_comm": search, "Locate_comm": 0.0, "Fetch_comm": fetch, "Total_comm": total}

    correctness: dict[tuple[int, int, str], dict[str, int]] = defaultdict(
        lambda: {"runs": 0, "search": 0, "fetch": 0}
    )
    for row in rows(RAW / "09_correctness.csv"):
        if row["row_type"] != "measured":
            continue
        key = (int(row["N"]), int(row["K"]), row["mode"].upper())
        correctness[key]["runs"] += 1
        correctness[key]["search"] += truth(row["selected_ids_match_exact_first_k"])
        correctness[key]["fetch"] += all(
            truth(row[name])
            for name in (
                "fetch_exact_reconstruction", "selected_windows_exact",
                "unselected_windows_zero", "unused_tail_zero",
            )
        )

    result: dict[tuple[int, int, str], dict[str, float | str]] = {}
    for ring, n in N_TO_N.items():
        for k in TARGET_K:
            for mode in MODES:
                group = [
                    values for (r, kk, mm, _run), values in procedure_groups.items()
                    if (r, kk, mm) == (ring, k, mode)
                ]
                assert len(group) == 20
                search = [v["MatchQuery"] + v["MatchResponse"] for v in group]
                locate = [v["Locate"] for v in group]
                fetch = [v["FetchQuery"] + v["FetchResponse"] + v["Recover"] for v in group]
                online = [
                    v["MatchQuery"] + v["MatchResponse"] + v["Locate"]
                    + v["FetchQuery"] + v["FetchResponse"] + v["Recover"]
                    for v in group
                ]
                ck = correctness[(ring, k, mode)]
                assert ck["runs"] == 20
                kvals = realized[(ring, k, mode)]
                k_text = str(min(kvals)) if min(kvals) == max(kvals) else f"{min(kvals)}--{max(kvals)}"
                values: dict[str, float | str] = {
                    "Setup": statistics.median(setup_groups[(ring, mode)]),
                    "Search": statistics.median(search),
                    "Locate": statistics.median(locate),
                    "Fetch": statistics.median(fetch),
                    # Important: median of per-run sums, not sum of displayed medians.
                    "Online": statistics.median(online),
                    "Search_correct": f"{ck['search']}/20",
                    "Fetch_correct": f"{ck['fetch']}/20",
                    "k": k_text,
                }
                values.update(comm[(ring, k, mode)])
                result[(n, k, f"CBPIR-{mode}")] = values
    return result


def clean_tex_cell(value: str) -> str:
    value = re.sub(r"\\cite(?:\[[^]]*\])?\{[^}]*\}", "", value)
    value = value.replace("$^\\dagger$", "").replace("$^\\dagger$", "")
    value = value.replace("$^\dagger$", "")
    value = value.replace("\\", "")
    value = re.sub(r"\\(?:textbf|textit|emph)\{([^{}]*)\}", r"\1", value)
    return " ".join(value.replace("$", "").split()).strip()


def parse_table() -> dict[tuple[int, int, str], dict[str, str]]:
    text = TEX.read_text(encoding="utf-8")
    start = text.index(r"\label{tab:performance}")
    end = text.index(r"\end{tabular}", start)
    target = text[start:end]
    current_n: int | None = None
    current_k: int | None = None
    parsed: dict[tuple[int, int, str], dict[str, str]] = {}
    for chunk in target.split(r"\\"):
        n_match = re.search(r"\\multirow\{12\}\{\*\}\{(\d+)\}", chunk)
        k_match = re.search(r"\\multirow\{3\}\{\*\}\{(\d+)\}", chunk)
        if n_match:
            current_n = int(n_match.group(1))
        if k_match:
            current_k = int(k_match.group(1))
        protocol_match = re.search(r"(BF-COIE(?:~\\cite\{[^}]+\})?|CBPIR-(?:FPS|RFPS))", chunk)
        if not protocol_match:
            continue
        assert current_n is not None and current_k is not None
        cells = [clean_tex_cell(part) for part in chunk.split("&")]
        protocol_index = next(i for i, cell in enumerate(cells) if "BF-COIE" in cell or "CBPIR-" in cell)
        protocol = "BF-COIE" if "BF-COIE" in cells[protocol_index] else cells[protocol_index]
        k_observed = cells[protocol_index - 1]
        fields = cells[protocol_index + 1 : protocol_index + 12]
        assert len(fields) == 11, (current_n, current_k, protocol, fields)
        names = (
            "Setup", "Search", "Locate", "Fetch", "Online",
            "Search_comm", "Locate_comm", "Fetch_comm", "Total_comm",
            "Search_correct", "Fetch_correct",
        )
        parsed[(current_n, current_k, protocol)] = {"k": k_observed, **dict(zip(names, fields))}
    assert len(parsed) == 36, len(parsed)
    return parsed


def numeric(value: str) -> float | None:
    try:
        return float(value)
    except ValueError:
        return None


def expected_decimals(metric: str, protocol: str) -> int | None:
    if metric in ("Search_correct", "Fetch_correct", "k"):
        return None
    if protocol == "BF-COIE" and metric in ("Locate", "Fetch", "Online"):
        return 1
    return 3


def build_reproduction() -> tuple[list[dict[str, object]], dict[str, object]]:
    target = parse_table()
    cb = cbpir_values()
    reconstructed: dict[tuple[int, int, str], dict[str, float | str]] = dict(cb)
    for n in N_TO_RING:
        for k in TARGET_K:
            reconstructed[(n, k, "BF-COIE")] = bf_values(n, k)

    output: list[dict[str, object]] = []
    verified = 0
    total = 0
    numerical_mismatches: list[dict[str, object]] = []
    for key in sorted(target, key=lambda x: (x[0], x[1], x[2])):
        n, k, protocol = key
        for metric, table_value in target[key].items():
            reproduced = reconstructed[key][metric]
            decimals = expected_decimals(metric, protocol)
            table_number = numeric(table_value)
            if table_number is None or isinstance(reproduced, str):
                reproduced_text = str(reproduced)
                status = "VERIFIED" if table_value == reproduced_text else "MISMATCH"
                absolute = relative = ""
            else:
                assert decimals is not None
                reproduced_text = f"{float(reproduced):.{decimals}f}"
                absolute_value = table_number - float(reproduced)
                relative_value = absolute_value / float(reproduced) if float(reproduced) else 0.0
                absolute = f"{absolute_value:.12g}"
                relative = f"{relative_value:.12g}"
                status = (
                    "VERIFIED"
                    if math.isclose(table_number, round(float(reproduced), decimals), abs_tol=10 ** (-(decimals + 2)))
                    else "ROUNDING_MISMATCH"
                )
            total += 1
            verified += status == "VERIFIED"
            provenance = "P+E" if protocol == "BF-COIE" else "D"
            evidence = (
                "Choi Fig.4+Algorithms 1/7+historical SealPIR model"
                if protocol == "BF-COIE"
                else "formal-seven-procedure-benchmark raw CSVs"
            )
            row = {
                "n": n, "K": k, "protocol": protocol, "metric": metric,
                "table_value": table_value, "reproduced_value": reproduced_text,
                "unrounded_reproduced_value": reproduced,
                "absolute_difference": absolute, "relative_difference": relative,
                "provenance": provenance, "status": status, "evidence": evidence,
            }
            output.append(row)
            if status != "VERIFIED":
                numerical_mismatches.append(row)

    bf = {(n, k): bf_values(n, k) for n in N_TO_RING for k in TARGET_K}
    def cb_values(mode: str, metric: str) -> list[float]:
        return [float(cb[(n, k, f"CBPIR-{mode}")][metric]) for n in N_TO_RING for k in TARGET_K]
    def bf_metric(metric: str) -> list[float]:
        return [float(bf[(n, k)][metric]) for n in N_TO_RING for k in TARGET_K]

    bf_online = bf_metric("Online")
    bf_comm = bf_metric("Total_comm")
    cb_online_all = cb_values("FPS", "Online") + cb_values("RFPS", "Online")
    cb_comm_all = cb_values("FPS", "Total_comm") + cb_values("RFPS", "Total_comm")
    bf_online_double = bf_online + bf_online
    bf_comm_double = bf_comm + bf_comm
    online_ratios = [a / b for a, b in zip(bf_online_double, cb_online_all)]
    comm_ratios = [a / b for a, b in zip(bf_comm_double, cb_comm_all)]

    ratios: dict[str, object] = {
        "cells_verified": verified,
        "cells_total": total,
        "mismatches": numerical_mismatches,
        "latency_ratio_aggregate": sum(bf_online_double) / sum(cb_online_all),
        "communication_ratio_aggregate": sum(bf_comm_double) / sum(cb_comm_all),
        "latency_ratio_mean_rows": statistics.mean(online_ratios),
        "latency_ratio_median_rows": statistics.median(online_ratios),
        "latency_ratio_min": min(online_ratios),
        "latency_ratio_max": max(online_ratios),
        "communication_ratio_mean_rows": statistics.mean(comm_ratios),
        "communication_ratio_median_rows": statistics.median(comm_ratios),
        "communication_ratio_min": min(comm_ratios),
        "communication_ratio_max": max(comm_ratios),
    }
    for mode in MODES:
        ratios[f"latency_ratio_{mode.lower()}"] = sum(bf_online) / sum(cb_values(mode, "Online"))
        ratios[f"communication_ratio_{mode.lower()}"] = sum(bf_comm) / sum(cb_values(mode, "Total_comm"))
    for n in N_TO_RING:
        bfo = sum(float(bf[(n, k)]["Online"]) for k in TARGET_K) * 2
        bfc = sum(float(bf[(n, k)]["Total_comm"]) for k in TARGET_K) * 2
        cbo = sum(float(cb[(n, k, f"CBPIR-{m}")]["Online"]) for m in MODES for k in TARGET_K)
        cbc = sum(float(cb[(n, k, f"CBPIR-{m}")]["Total_comm"]) for m in MODES for k in TARGET_K)
        ratios[f"latency_ratio_n{n}"] = bfo / cbo
        ratios[f"communication_ratio_n{n}"] = bfc / cbc
    for k in TARGET_K:
        bfo = sum(float(bf[(n, k)]["Online"]) for n in N_TO_RING) * 2
        bfc = sum(float(bf[(n, k)]["Total_comm"]) for n in N_TO_RING) * 2
        cbo = sum(float(cb[(n, k, f"CBPIR-{m}")]["Online"]) for m in MODES for n in N_TO_RING)
        cbc = sum(float(cb[(n, k, f"CBPIR-{m}")]["Total_comm"]) for m in MODES for n in N_TO_RING)
        ratios[f"latency_ratio_K{k}"] = bfo / cbo
        ratios[f"communication_ratio_K{k}"] = bfc / cbc

    # Shared downstream boundary: remove CBPIR Search from denominator.
    cb_downstream = cb_values("FPS", "Locate") + cb_values("RFPS", "Locate")
    cb_fetch = cb_values("FPS", "Fetch") + cb_values("RFPS", "Fetch")
    ratios["latency_ratio_shared_locate_fetch"] = sum(bf_online_double) / (
        sum(cb_downstream) + sum(cb_fetch)
    )
    cb_fetch_comm = cb_values("FPS", "Fetch_comm") + cb_values("RFPS", "Fetch_comm")
    ratios["communication_ratio_shared_locate_fetch"] = sum(bf_comm_double) / sum(cb_fetch_comm)

    # Conservative baseline-favoring central sensitivity: smallest documented
    # BF model bounds, omit uncertain n=512 geometry factor, and divide by the
    # slower measured CBPIR variant in each workload.  This is not a new central
    # estimate; it is an adversarial lower-bound stress test.
    conservative_bf_latency = 0.0
    conservative_cb_latency = 0.0
    conservative_bf_comm = 0.0
    conservative_cb_comm = 0.0
    for n in N_TO_RING:
        for k in TARGET_K:
            cell = bf[(n, k)]
            conservative_bf_latency += float(cell["locate_low_ms"]) + min(
                float(cell["fetch_origin_ms"]), float(cell["fetch_intercept_ms"]),
                float(cell["fetch_without_geometry_ms"]),
            )
            conservative_cb_latency += max(
                float(cb[(n, k, "CBPIR-FPS")]["Online"]),
                float(cb[(n, k, "CBPIR-RFPS")]["Online"]),
            )
            conservative_bf_comm += float(cell["locate_comm_floor_mib"]) + float(cell["Fetch_comm"])
            conservative_cb_comm += max(
                float(cb[(n, k, "CBPIR-FPS")]["Total_comm"]),
                float(cb[(n, k, "CBPIR-RFPS")]["Total_comm"]),
            )
    ratios["latency_ratio_conservative"] = conservative_bf_latency / conservative_cb_latency
    ratios["communication_ratio_conservative"] = conservative_bf_comm / conservative_cb_comm

    # Isolate the n=512 geometry assumption.
    central_sum = sum(bf_online)
    no_factor_sum = sum(
        float(bf[(n, k)]["Locate"]) + float(bf[(n, k)]["fetch_without_geometry_ms"])
        for n in N_TO_RING for k in TARGET_K
    )
    cb_average_sum = sum(cb_online_all) / 2
    ratios["latency_ratio_without_15_14"] = no_factor_sum / cb_average_sum
    ratios["latency_ratio_central_single_matrix"] = central_sum / cb_average_sum
    return output, ratios


def main() -> None:
    output, summary = build_reproduction()
    with OUT.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(output[0]))
        writer.writeheader()
        writer.writerows(output)
    for name, value in summary.items():
        if name == "mismatches":
            print(f"{name}={len(value)}")
            for row in value:
                print(
                    "  " + ", ".join(
                        f"{field}={row[field]}"
                        for field in ("n", "K", "protocol", "metric", "table_value", "reproduced_value", "status")
                    )
                )
        elif isinstance(value, float):
            print(f"{name}={value:.12g}")
        else:
            print(f"{name}={value}")


if __name__ == "__main__":
    main()
