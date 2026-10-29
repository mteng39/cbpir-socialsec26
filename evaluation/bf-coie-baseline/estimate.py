#!/usr/bin/env python3
"""Generate paper-calibrated BF-COIE estimates for the 12 matched workloads.

No local BF-COIE timing enters this model. Published Figure 4 costs calibrate
the computational terms; Algorithms 1, 2, and 7 provide structural counts.
"""

from __future__ import annotations

import csv
from dataclasses import asdict, dataclass
import math
from pathlib import Path
from statistics import mean
from typing import Iterable


FP = 16
ETA = 2
LAMBDA_BITS = 40
ITEM_BYTES = 64
CBPIR_N = {128: 8192, 256: 16384, 512: 32768}
FHE_CIPHERTEXT_APPROX_BYTES = 103_000
COMMUNICATION_BASIS = "published-formula-derived estimate"
COMMUNICATION_LIMITATION = (
    "Figure 5 reports 1323 BF-COIE ciphertexts at n=10000,s=16, but this "
    "count cannot be reconciled with the published BF-COIE formulas or "
    "recovered source material."
)
LOCATE_CENTRAL_MODEL = "floor-hierarchy through-origin"
FETCH_EXTRAPOLATION_MODEL = "request-count intercept fit"
FETCH_ANCHOR_MODEL = "Figure 4 Retrieval anchor"
FETCH_N512_CENTRAL_FACTOR = 15 / 14
FETCH_N512_FACTOR_LOW = 1.0
FETCH_N512_FACTOR_HIGH = 1.5
FETCH_N512_DECISION = "retain 15/14 with parameter sensitivity"

# Figure 4 was extracted from the embedded 2000x1400 source image. Its vertical
# scale is 7.2 pixels/second. Values are approximate, not textual measurements.
PAPER_S = (8, 16, 32, 64, 128)
FIG4_SERVER_SECONDS = tuple(value / 7.2 for value in (105, 94, 84, 72, 60))
FIG4_CLIENT_SECONDS = tuple(value / 7.2 for value in (2, 4, 8, 18, 33))
FIG4_RETRIEVAL_SECONDS = tuple(value / 7.2 for value in (9, 14, 23, 42, 80))

# Figure 3 values used only as an external fit/sensitivity check. The 10,000
# point is exact prose; later points are digitized marker centers.
FIG3_N = (10_000, 20_000, 30_000, 40_000, 50_000, 60_000, 70_000, 80_000, 90_000, 100_000)
FIG3_TOTAL_SECONDS = (
    16.7, 21.95, 35.31, 48.66, 62.02, 75.38, 88.74, 102.10, 117.37, 132.63
)


@dataclass(frozen=True)
class StructuralCell:
    n: int
    s: int
    hierarchy_t: int
    bf_levels: int
    m: int
    ell_raw: float
    ell: int
    compactness: int
    count_additions: int
    encode_hashes: int
    encode_additions_lower: int
    encode_additions_upper: int
    decode_check_bound: int
    pir_requests: int
    ct_ct_multiplications: int


@dataclass(frozen=True)
class EstimateCell:
    n: int
    s: int
    cbpir_N: int
    bfcoie_N: None
    logical_item_bytes: int
    database_bytes: int
    hierarchy_t: int
    bf_levels: int
    m: int
    ell_raw: float
    ell_floor: int
    ell_ceil: int
    ell: int
    compactness: int
    locate_ciphertexts_floor: int
    locate_ciphertexts_ceil: int
    count_additions: int
    encode_hashes: int
    decode_check_bound: int
    pir_requests: int
    ct_ct_multiplications: int
    locate_ms: float
    locate_low_ms: float
    locate_high_ms: float
    locate_central_model: str
    fetch_ms: float
    fetch_low_ms: float
    fetch_high_ms: float
    fetch_central_model: str
    fetch_geometry_factor: float
    fetch_geometry_factor_low: float
    fetch_geometry_factor_high: float
    fetch_geometry_decision: str
    postmatch_ms: float
    locate_communication_mib: float
    locate_communication_low_mib: float
    locate_communication_high_mib: float
    fetch_communication_mib: float
    postmatch_communication_mib: float
    locate_provenance: str
    fetch_provenance: str
    communication_provenance: str
    communication_basis: str
    communication_limitation: str
    fairly_comparable: bool
    fairness_note: str


def structural_cell(n: int, s: int) -> StructuralCell:
    if n not in CBPIR_N or s not in (1, 4, 8, 16):
        raise ValueError("cell is outside the controlled comparison matrix")
    ratio = n // (2 * s)
    if ratio * 2 * s != n or ratio & (ratio - 1):
        raise ValueError("exact power-of-two hierarchy required")
    hierarchy_t = ratio.bit_length() - 1
    levels = hierarchy_t + 1
    m = max(2 * s, s + 2 * FP)
    ell_raw = ETA * s * math.sqrt(m)
    ell = math.ceil(ell_raw)
    compactness = levels * ell
    attempts = ETA * n * levels
    return StructuralCell(
        n=n,
        s=s,
        hierarchy_t=hierarchy_t,
        bf_levels=levels,
        m=m,
        ell_raw=ell_raw,
        ell=ell,
        compactness=compactness,
        count_additions=n - 1,
        encode_hashes=attempts,
        encode_additions_lower=max(0, attempts - compactness),
        encode_additions_upper=attempts,
        decode_check_bound=2 * s + hierarchy_t * m,
        pir_requests=s + FP,
        ct_ct_multiplications=0,
    )


def _linear_fit(xs: Iterable[float], ys: Iterable[float], intercept: bool) -> tuple[float, float, float]:
    x = tuple(xs)
    y = tuple(ys)
    if len(x) != len(y) or not x:
        raise ValueError("fit inputs must have equal nonzero length")
    if intercept:
        x_mean = mean(x)
        y_mean = mean(y)
        slope = sum((a - x_mean) * (b - y_mean) for a, b in zip(x, y)) / sum(
            (a - x_mean) ** 2 for a in x
        )
        offset = y_mean - slope * x_mean
    else:
        slope = sum(a * b for a, b in zip(x, y)) / sum(a * a for a in x)
        offset = 0.0
    residuals = [slope * a + offset - b for a, b in zip(x, y)]
    rmse = math.sqrt(mean(value * value for value in residuals))
    return slope, offset, rmse


def _paper_structure(s: int, hierarchy: str) -> tuple[int, int]:
    raw = math.log2(10_000 / (2 * s))
    hierarchy_t = math.floor(raw) if hierarchy == "floor" else math.ceil(raw)
    levels = hierarchy_t + 1
    ell = math.ceil(ETA * s * math.sqrt(max(2 * s, s + 2 * FP)))
    return ETA * 10_000 * levels, levels * ell


def model_fits() -> dict[str, tuple[float, float, float]]:
    result: dict[str, tuple[float, float, float]] = {}
    for hierarchy in ("floor", "ceil"):
        attempts = [_paper_structure(s, hierarchy)[0] for s in PAPER_S]
        compactness = [_paper_structure(s, hierarchy)[1] for s in PAPER_S]
        for intercept in (False, True):
            suffix = "intercept" if intercept else "origin"
            result[f"server_{hierarchy}_{suffix}"] = _linear_fit(
                attempts, FIG4_SERVER_SECONDS, intercept
            )
            result[f"client_{hierarchy}_{suffix}"] = _linear_fit(
                compactness, FIG4_CLIENT_SECONDS, intercept
            )
    requests = [s + FP for s in PAPER_S]
    result["fetch_origin"] = _linear_fit(requests, FIG4_RETRIEVAL_SECONDS, False)
    result["fetch_intercept"] = _linear_fit(requests, FIG4_RETRIEVAL_SECONDS, True)
    result["figure3_total_origin"] = _linear_fit(FIG3_N, FIG3_TOTAL_SECONDS, False)
    result["figure3_total_intercept"] = _linear_fit(FIG3_N, FIG3_TOTAL_SECONDS, True)
    return result


def _locate_estimate(structure: StructuralCell) -> tuple[float, float, float]:
    fits = model_fits()
    origin_values: dict[str, float] = {}
    for hierarchy in ("floor", "ceil"):
        server_slope, _, _ = fits[f"server_{hierarchy}_origin"]
        client_slope, _, _ = fits[f"client_{hierarchy}_origin"]
        origin_values[hierarchy] = (
            server_slope * structure.encode_hashes
            + client_slope * structure.compactness
        )
    central = origin_values["floor"]
    return central, min(origin_values.values()), max(origin_values.values())


def _fetch_estimate(
    n: int, s: int
) -> tuple[float, float, float, str, float, float, float, str]:
    fits = model_fits()
    requests = s + FP
    through_origin = fits["fetch_origin"][0] * requests
    with_intercept = max(0.0, fits["fetch_intercept"][0] * requests + fits["fetch_intercept"][1])
    if s in PAPER_S:
        observed = FIG4_RETRIEVAL_SECONDS[PAPER_S.index(s)]
        central = observed
        low = min(observed, through_origin, with_intercept)
        high = max(observed, through_origin, with_intercept)
        central_model = FETCH_ANCHOR_MODEL
    else:
        central = with_intercept
        low = min(through_origin, with_intercept)
        high = max(through_origin, with_intercept)
        central_model = FETCH_EXTRAPOLATION_MODEL

    # Historical SealPIR defaults (N=4096, logt=20, d=2) pad the paper's
    # 10,000x2-byte DB to [2,2]. The 128x64 and 256x64 DBs also use [2,2].
    # The 512x64 DB uses [3,2]. With default expansion ratio 12, the exact
    # ct-pt multiplication ratio is (6+2R)/(4+2R)=15/14. Authors' undisclosed
    # PIR parameters justify a conservative [1,1.5] sensitivity interval.
    if n == 512:
        central_factor = FETCH_N512_CENTRAL_FACTOR
        low_factor = FETCH_N512_FACTOR_LOW
        high_factor = FETCH_N512_FACTOR_HIGH
        geometry_decision = FETCH_N512_DECISION
    else:
        central_factor = low_factor = high_factor = 1.0
        geometry_decision = "no geometry adjustment"
    return (
        central * central_factor,
        low * low_factor,
        high * high_factor,
        central_model,
        central_factor,
        low_factor,
        high_factor,
        geometry_decision,
    )


def build_cells() -> list[EstimateCell]:
    cells = []
    for n in (128, 256, 512):
        for s in (1, 4, 8, 16):
            structure = structural_cell(n, s)
            locate_s, locate_low_s, locate_high_s = _locate_estimate(structure)
            (
                fetch_s,
                fetch_low_s,
                fetch_high_s,
                fetch_central_model,
                fetch_geometry_factor,
                fetch_geometry_factor_low,
                fetch_geometry_factor_high,
                fetch_geometry_decision,
            ) = _fetch_estimate(n, s)
            ell_floor = math.floor(structure.ell_raw)
            ell_ceil = math.ceil(structure.ell_raw)
            locate_ciphertexts_floor = 1 + structure.bf_levels * ell_floor
            locate_ciphertexts_ceil = 1 + structure.bf_levels * ell_ceil
            locate_low_mib = (
                locate_ciphertexts_floor * FHE_CIPHERTEXT_APPROX_BYTES / (1024 * 1024)
            )
            locate_high_mib = (
                locate_ciphertexts_ceil * FHE_CIPHERTEXT_APPROX_BYTES / (1024 * 1024)
            )
            locate_mib = locate_high_mib
            fetch_mib = structure.pir_requests * 368_600 / (1024 * 1024)
            cells.append(
                EstimateCell(
                    n=n,
                    s=s,
                    cbpir_N=CBPIR_N[n],
                    bfcoie_N=None,
                    logical_item_bytes=ITEM_BYTES,
                    database_bytes=n * ITEM_BYTES,
                    hierarchy_t=structure.hierarchy_t,
                    bf_levels=structure.bf_levels,
                    m=structure.m,
                    ell_raw=structure.ell_raw,
                    ell_floor=ell_floor,
                    ell_ceil=ell_ceil,
                    ell=structure.ell,
                    compactness=structure.compactness,
                    locate_ciphertexts_floor=locate_ciphertexts_floor,
                    locate_ciphertexts_ceil=locate_ciphertexts_ceil,
                    count_additions=structure.count_additions,
                    encode_hashes=structure.encode_hashes,
                    decode_check_bound=structure.decode_check_bound,
                    pir_requests=structure.pir_requests,
                    ct_ct_multiplications=0,
                    locate_ms=locate_s * 1000,
                    locate_low_ms=locate_low_s * 1000,
                    locate_high_ms=locate_high_s * 1000,
                    locate_central_model=LOCATE_CENTRAL_MODEL,
                    fetch_ms=fetch_s * 1000,
                    fetch_low_ms=fetch_low_s * 1000,
                    fetch_high_ms=fetch_high_s * 1000,
                    fetch_central_model=fetch_central_model,
                    fetch_geometry_factor=fetch_geometry_factor,
                    fetch_geometry_factor_low=fetch_geometry_factor_low,
                    fetch_geometry_factor_high=fetch_geometry_factor_high,
                    fetch_geometry_decision=fetch_geometry_decision,
                    postmatch_ms=(locate_s + fetch_s) * 1000,
                    locate_communication_mib=locate_mib,
                    locate_communication_low_mib=locate_low_mib,
                    locate_communication_high_mib=locate_high_mib,
                    fetch_communication_mib=fetch_mib,
                    postmatch_communication_mib=locate_mib + fetch_mib,
                    locate_provenance="F+X",
                    fetch_provenance="D+F+X",
                    communication_provenance="P+D+X",
                    communication_basis=COMMUNICATION_BASIS,
                    communication_limitation=COMMUNICATION_LIMITATION,
                    fairly_comparable=True,
                    fairness_note=(
                        "same n, 64-byte item, K=s; fp=16 retained; workload matched, "
                        "not HE-ring matched"
                    ),
                )
            )
    return cells


def _write_csv(path: Path, rows: list[dict[str, object]], fields: list[str] | None = None) -> None:
    if not rows:
        raise ValueError(f"no rows for {path}")
    selected = fields or list(rows[0])
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=selected, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def write_generated_outputs(output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    cells = build_cells()
    structures = [asdict(structural_cell(n, s)) for n in (128, 256, 512) for s in (1, 4, 8, 16)]
    for row in structures:
        row["provenance"] = "D"
        row["ell_rounding"] = "ceil-local-convention"
    _write_csv(output / "04_structural_matrix.csv", structures)

    fit_rows = []
    for name, (slope, intercept, rmse) in model_fits().items():
        source = (
            "Choi et al. Figure 3 (text anchor plus digitized points)"
            if name.startswith("figure3_")
            else "Choi et al. Figure 4 (digitized)"
        )
        if name in ("server_floor_origin", "client_floor_origin"):
            fit_role = "Locate central component"
        elif name in ("server_ceil_origin", "client_ceil_origin"):
            fit_role = "Locate hierarchy sensitivity component"
        elif name in ("server_floor_intercept", "client_floor_intercept", "server_ceil_intercept", "client_ceil_intercept"):
            fit_role = "Locate diagnostic only; unsafe downward extrapolation"
        elif name == "fetch_intercept":
            fit_role = "Fetch central for K=1,4; sensitivity otherwise"
        elif name == "fetch_origin":
            fit_role = "Fetch sensitivity"
        else:
            fit_role = "external validation only"
        fit_rows.append(
            {
                "model": name,
                "slope": f"{slope:.12g}",
                "intercept_seconds": f"{intercept:.12g}",
                "rmse_seconds": f"{rmse:.12g}",
                "source": source,
                "provenance": "F",
                "fit_role": fit_role,
            }
        )
    _write_csv(output / "05_model_fits.csv", fit_rows)

    estimate_rows = [asdict(cell) for cell in cells]
    latency_fields = [
        "n", "s", "cbpir_N", "logical_item_bytes", "database_bytes",
        "locate_ms", "locate_low_ms", "locate_high_ms", "locate_central_model",
        "fetch_ms", "fetch_low_ms", "fetch_high_ms", "fetch_central_model",
        "fetch_geometry_factor", "fetch_geometry_factor_low",
        "fetch_geometry_factor_high", "fetch_geometry_decision", "postmatch_ms",
        "locate_provenance", "fetch_provenance",
    ]
    _write_csv(output / "08_latency_estimates.csv", estimate_rows, latency_fields)
    communication_fields = [
        "n", "s", "cbpir_N", "bf_levels", "m", "ell_raw", "ell_floor",
        "ell_ceil", "locate_ciphertexts_floor", "locate_ciphertexts_ceil",
        "compactness", "pir_requests",
        "locate_communication_mib", "locate_communication_low_mib",
        "locate_communication_high_mib", "fetch_communication_mib",
        "postmatch_communication_mib", "communication_basis",
        "communication_limitation", "communication_provenance",
    ]
    _write_csv(output / "09_communication_estimates.csv", estimate_rows, communication_fields)

    sensitivity = []
    for cell in cells:
        sensitivity.extend(
            [
                {
                    "n": cell.n,
                    "s": cell.s,
                    "quantity": "Locate_ms",
                    "central": cell.locate_ms,
                    "low": cell.locate_low_ms,
                    "high": cell.locate_high_ms,
                    "source_of_range": (
                        "floor versus ceiling paper hierarchy calibration; both use "
                        "through-origin fits; intercept fits are diagnostic only"
                    ),
                    "provenance": "F+X",
                },
                {
                    "n": cell.n,
                    "s": cell.s,
                    "quantity": "Fetch_ms",
                    "central": cell.fetch_ms,
                    "low": cell.fetch_low_ms,
                    "high": cell.fetch_high_ms,
                    "source_of_range": (
                        "K=1,4 intercept versus origin request-count fits; K=8,16 "
                        "Figure 4 anchors versus fits; n=512 also includes "
                        "parameter-dependent SealPIR geometry factor 1.0 to 1.5"
                    ),
                    "provenance": "D+F+X",
                },
                {
                    "n": cell.n,
                    "s": cell.s,
                    "quantity": "Locate_communication_MiB",
                    "central": cell.locate_communication_mib,
                    "low": cell.locate_communication_low_mib,
                    "high": cell.locate_communication_high_mib,
                    "source_of_range": (
                        "floor/ceiling ell rounding only; Figure 5 count is "
                        "unresolved and is not used as a correction"
                    ),
                    "provenance": "P+D+X",
                },
            ]
        )
    _write_csv(output / "07_sensitivity.csv", sensitivity)

    correctness = []
    fairness = []
    proposal = []
    for cell in cells:
        correctness.append(
            {
                "n": cell.n,
                "s": cell.s,
                "no_false_negative_guarantee": True,
                "false_positive_bound": FP,
                "abort_if_false_positives_gt_fp": True,
                "pir_correctness": "assumed from SealPIR",
                "dummy_requests": FP,
                "provenance": "P+D",
            }
        )
        fairness.append(
            {
                "n": cell.n,
                "K": cell.s,
                "cbpir_N": cell.cbpir_N,
                "logical_item_bytes": ITEM_BYTES,
                "bfcoie_s_equals_K": True,
                "fp_retained": FP,
                "pir_requests": cell.pir_requests,
                "ring_equivalence_claimed": False,
                "raw_2byte_fetch_used_directly": False,
                "locate_boundary_aligned": True,
                "fetch_boundary_aligned": True,
                "logical_workload_comparability": "COMPARABLE WITH ESTIMATION LIMITS",
                "execution_environment_match": "NO",
                "execution_environment_note": (
                    "BF-COIE latency is calibrated from Choi et al.'s i9-9900K; "
                    "CBPIR is measured on an AMD EPYC virtual benchmark host"
                ),
                "computation_comparison_scope": (
                    "CROSS-PLATFORM; NOT HARDWARE-NORMALIZED"
                ),
            }
        )
        proposal.append(
            {
                "group": f"{cell.n} (2^{int(math.log2(cell.cbpir_N))})",
                "K": cell.s,
                "k": cell.s,
                "protocol": "BF-COIE estimate",
                "Setup_ms": "--",
                "Search_ms": "--",
                "Locate_ms": f"{cell.locate_ms:.1f}",
                "Fetch_ms": f"{cell.fetch_ms:.1f}",
                "Online_ms": "--",
                "Search_comm_MiB": "--",
                "Locate_comm_MiB": f"{cell.locate_communication_mib:.3f}",
                "Locate_comm_floor_MiB": f"{cell.locate_communication_low_mib:.3f}",
                "Locate_comm_ceil_MiB": f"{cell.locate_communication_high_mib:.3f}",
                "Fetch_comm_MiB": f"{cell.fetch_communication_mib:.3f}",
                "PostMatch_comm_MiB": f"{cell.postmatch_communication_mib:.3f}",
                "communication_basis": cell.communication_basis,
                "locate_central_model": cell.locate_central_model,
                "fetch_central_model": cell.fetch_central_model,
                "fetch_geometry_factor": f"{cell.fetch_geometry_factor:.12g}",
                "execution_environment_match": "NO",
                "provenance": (
                    "Locate F+X; Fetch D+F+X; communication P+D+X; "
                    "Figure 5 count unresolved"
                ),
            }
        )
    _write_csv(output / "10_correctness.csv", correctness)
    _write_csv(output / "11_fairness_audit.csv", fairness)
    _write_csv(output / "12_table2_proposal.csv", proposal)


if __name__ == "__main__":
    write_generated_outputs(Path(__file__).resolve().parent)
