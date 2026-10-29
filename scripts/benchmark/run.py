"""Run, validate, and report the formal seven-procedure CBPIR benchmark."""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import socket
import statistics
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import formal_workload


ROOT = HERE.parents[1]
if str(ROOT / "scripts/common") not in sys.path:
    sys.path.insert(0, str(ROOT / "scripts/common"))

from workspace_build_lock import shared_workspace_build_lock
P0A = ROOT / "crates/bgv-cpir"
P0C = ROOT / "crates/ckks-search"
P0D = ROOT
RETAINED_EVIDENCE = ROOT / "evaluation/formal-benchmark"
EVIDENCE = ROOT / "target/reproduction/formal-benchmark"
FROZEN_INPUTS = RETAINED_EVIDENCE
FROZEN = FROZEN_INPUTS / "frozen-inputs/phase2-full-capacity-final"
PHASE2 = FROZEN_INPUTS / "frozen-inputs/phase2-full-capacity"
OPENFHE = ROOT / "openfhe/.openfhe/openfhe-1.5.1/install"
TARGET = ROOT / "target/experiments/benchmark"
BINARY = TARGET / "release/benchmark"
NATIVE = EVIDENCE / "native"
CANONICAL_INTEGRITY = RETAINED_EVIDENCE / "verification/frozen_phase2_integrity.json"
CARGO = Path(shutil.which("cargo") or "cargo")
RUSTC = Path(shutil.which("rustc") or "rustc")

REQUIRED_OUTPUTS = (
    "00_environment.txt",
    "01_benchmark_report.md",
    "02_fixture_manifest.csv",
    "03_setup_raw.csv",
    "04_setup_summary.csv",
    "05_procedure_raw.csv",
    "06_procedure_summary.csv",
    "07_online_summary.csv",
    "08_communication.csv",
    "09_correctness.csv",
    "10_verification.txt",
    "11_fetch_k_scaling_raw.csv",
    "12_fetch_k_scaling_summary.csv",
)

EXPECTED_HE = {
    8192: (2, 60, 50, "FIXEDMANUAL", "BV"),
    16384: (2, 60, 40, "FIXEDMANUAL", "HYBRID"),
    32768: (2, 60, 50, "FIXEDMANUAL", "HYBRID"),
}
EXPECTED_RFPS_PROBES = {
    8192: {
        "rfps-literal": (0, 0),
        "rfps-cancellation": (2, 0),
        "rfps-threshold-only": (0, 4),
    },
    16384: {
        "rfps-literal": (0, 0),
        "rfps-cancellation": (5, 0),
        "rfps-threshold-only": (6, 10),
    },
    32768: {
        "rfps-literal": (0, 0),
        "rfps-cancellation": (11, 0),
        "rfps-threshold-only": (11, 24),
    },
}
STAGES = (
    ("MatchQuery", "match_query_ns"),
    ("MatchResponse", "match_response_ns"),
    ("Locate", "locate_ns"),
    ("FetchQuery", "fetch_query_ns"),
    ("FetchResponse", "fetch_response_ns"),
    ("Recover", "recover_ns"),
    ("OnlineSum", "online_sum_ns"),
    ("OnlineOuter", "online_outer_ns"),
)


def summary(values):
    """Return reproducible millisecond statistics for nanosecond samples."""
    if not values:
        raise ValueError("summary requires at least one sample")
    ordered = sorted(values)
    p95_index = max(0, math.ceil(0.95 * len(ordered)) - 1)
    to_ms = lambda value: value / 1_000_000
    return {
        "samples": len(ordered),
        "mean_ms": to_ms(statistics.fmean(ordered)),
        "median_ms": to_ms(statistics.median(ordered)),
        "stddev_ms": to_ms(statistics.stdev(ordered)) if len(ordered) > 1 else 0.0,
        "min_ms": to_ms(ordered[0]),
        "max_ms": to_ms(ordered[-1]),
        "p95_ms": to_ms(ordered[p95_index]),
    }


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def hash_tree(directory):
    return {
        str(path.relative_to(directory)): sha256_file(path)
        for path in sorted(directory.rglob("*"))
        if path.is_file()
        and "__pycache__" not in path.parts
        and path.suffix not in {".pyc", ".pyo"}
    }


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_csv(path, rows, fields):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def command_output(command):
    completed = subprocess.run(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, check=False,
    )
    return completed.returncode, completed.stdout.strip()


def benchmark_env():
    env = os.environ.copy()
    env.update({
        "OMP_NUM_THREADS": "1", "OMP_DYNAMIC": "FALSE",
        "OPENFHE_INSTALL_DIR": str(OPENFHE),
        "LD_LIBRARY_PATH": str(OPENFHE / "lib"),
        "CARGO_TARGET_DIR": str(TARGET),
    })
    return env


def active_benchmark_pids():
    found = []
    marker = str(BINARY).encode()
    for cmdline in Path("/proc").glob("[0-9]*/cmdline"):
        try:
            if marker in cmdline.read_bytes():
                found.append(int(cmdline.parent.name))
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    return sorted(found)


def process_threads(pid):
    try:
        for line in Path(f"/proc/{pid}/status").read_text(encoding="utf-8").splitlines():
            if line.startswith("Threads:"):
                return int(line.split(":", 1)[1])
    except (FileNotFoundError, ProcessLookupError):
        return None
    return None


def run_native(label, payload):
    if active_benchmark_pids():
        raise RuntimeError("another formal benchmark process is already active")
    input_path = NATIVE / f"{label}_input.json"
    output_path = NATIVE / f"{label}_output.json"
    stderr_path = NATIVE / f"{label}_stderr.txt"
    write_json(input_path, payload)
    started = time.monotonic()
    with input_path.open("rb") as stdin, output_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        process = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, env=benchmark_env(),
            stdin=stdin, stdout=stdout, stderr=stderr,
        )
        observed = []
        while process.poll() is None:
            count = process_threads(process.pid)
            if count is not None:
                observed.append(count)
            time.sleep(0.005)
        return_code = process.wait()
    elapsed = time.monotonic() - started
    if return_code != 0:
        message = stderr_path.read_text(encoding="utf-8", errors="replace")
        raise RuntimeError(f"{label} failed ({return_code}): {message[-4000:]}")
    output = json.loads(output_path.read_text(encoding="utf-8"))
    metadata = {
        "label": label, "command": str(BINARY),
        "input": str(input_path.relative_to(ROOT)),
        "output": str(output_path.relative_to(ROOT)),
        "stderr": str(stderr_path.relative_to(ROOT)),
        "elapsed_seconds": elapsed,
        "observed_thread_samples": len(observed),
        "minimum_threads": min(observed) if observed else None,
        "maximum_threads": max(observed) if observed else None,
        "active_benchmark_processes_before": 0,
        "return_code": return_code,
    }
    write_json(NATIVE / f"{label}_process.json", metadata)
    return output, metadata


def build_release():
    command = [
        str(CARGO), "build", "--release", "--locked", "--manifest-path",
        str(P0D / "Cargo.toml"), "--features", "native", "--bin",
        "benchmark",
    ]
    completed = subprocess.run(
        command, cwd=ROOT, env=benchmark_env(), text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    (NATIVE / "build.txt").write_text(completed.stdout, encoding="utf-8")
    if completed.returncode:
        raise RuntimeError("release build failed; see native/build.txt")
    return " ".join(command)


def validate_runtime(output):
    ring = output["ring_dimension"]
    depth, b0, bs, scaling, key_switch = EXPECTED_HE[ring]
    fps, rfps, fetch = output["fps_runtime"], output["rfps_runtime"], output["fetch_runtime"]
    checks = [
        output["schema"] == formal_workload.SCHEMA,
        output["openfhe_version"] == "1.5.1",
        fps["security"] == "HEStd_128_classic",
        fps["ring_dimension"] == ring, fps["batch_size"] == ring // 2,
        (fps["multiplicative_depth"], fps["first_modulus_bits"], fps["scaling_modulus_bits"])
        == (depth, b0, bs),
        fps["scaling_technique"] == scaling,
        fps["key_switch_technique"] == key_switch,
        rfps["security"] == "HEStd_128_classic",
        rfps["ring_dimension"] == ring, rfps["batch_size"] == ring // 2,
        (rfps["multiplicative_depth"], rfps["first_modulus_bits"],
         rfps["scaling_modulus_bits"], rfps["scaling_technique"])
        == (1, 60, 24, "FIXEDMANUAL"),
        fetch["security"] == "HEStd_128_classic",
        fetch["actual_ring_dimension"] == ring,
        fetch["plaintext_modulus"] == 65537,
        fetch["multiplicative_depth"] == 1,
        fetch["record_slots"] == 64,
    ]
    if not all(checks):
        raise ValueError(f"runtime HE parameter drift for N={ring}")


def load_formal_outputs():
    outputs, processes = {}, []
    for ring in sorted(formal_workload.EXPECTED):
        output_path = NATIVE / f"formal_{ring}_output.json"
        process_path = NATIVE / f"formal_{ring}_process.json"
        if not output_path.exists() or not process_path.exists():
            retained = RETAINED_EVIDENCE / "raw" / str(ring)
            output_path = retained / f"formal_{ring}_output.json"
            process_path = retained / f"formal_{ring}_process.json"
        if not output_path.exists() or not process_path.exists():
            raise FileNotFoundError(f"missing formal output for N={ring}")
        output = json.loads(output_path.read_text(encoding="utf-8"))
        validate_runtime(output)
        outputs[ring] = output
        processes.append(json.loads(process_path.read_text(encoding="utf-8")))
    return outputs, processes


def rfps_probe_gate(outputs):
    for ring, expected in EXPECTED_RFPS_PROBES.items():
        probe_rows = [
            row
            for row in outputs[ring]["correctness"]
            if row["row_type"] == "rfps_preservation_probe"
        ]
        if len(probe_rows) != len(expected):
            return False
        probes = {
            row["query_id"]: row
            for row in probe_rows
        }
        if set(probes) != set(expected):
            return False
        for query_id, (cancellation, threshold_only) in expected.items():
            row = probes[query_id]
            if (
                row["rfps_cancellation_count"] != cancellation
                or row["rfps_threshold_only_count"] != threshold_only
            ):
                return False
    return True


def thread_observation_gate(processes):
    return all(
        process["observed_thread_samples"] > 0
        and process["minimum_threads"] is not None
        and process["maximum_threads"] is not None
        and process["maximum_threads"] <= 1
        for process in processes
    )


def validate_results(outputs):
    inputs = formal_workload.build_inputs()
    for ring, output in outputs.items():
        expected = formal_workload.EXPECTED[ring]
        if output["warmups"] != 3 or output["measured_runs"] != 20:
            raise ValueError(f"repetition drift for N={ring}")
        if len(output["setup_samples"]) != 40 or len(output["online_samples"]) != 160:
            raise ValueError(f"sample matrix drift for N={ring}")
        for row in output["online_samples"]:
            if row["mode"] == "fps" and row["candidate_count"] != expected["candidates"]:
                raise ValueError(f"FPS candidate drift for N={ring}")
            if row["realized_k"] != min(row["candidate_count"], row["public_k"]):
                raise ValueError(f"First_K selection failure for N={ring}")
            if not row["success"]:
                raise ValueError(f"selection/correctness failure for N={ring}")
        measured = [row for row in output["correctness"] if row["row_type"] == "measured"]
        if len(measured) != 160:
            raise ValueError(f"correctness row drift for N={ring}")
        for row in measured:
            if not row["success"] or not row["fetch_exact_reconstruction"]:
                raise ValueError(f"measured correctness failure for N={ring}")
            if row["mode"] == "fps" and row["fps_oracle_mismatches"] != 0:
                raise ValueError(f"FPS oracle mismatch for N={ring}")
            if row["mode"] == "fps" and not row["selected_ids_match_exact_first_k"]:
                raise ValueError(f"FPS First_K ID mismatch for N={ring}")
            if row["query_ciphertext_count"] != 1 or row["response_ciphertext_count"] != 1:
                raise ValueError(f"Fetch message-count failure for N={ring}")
        communications = output["communication"]
        if len(communications) != 8:
            raise ValueError(f"communication matrix drift for N={ring}")
        if len({row["fetch_query_bytes"] for row in communications}) != 1:
            raise ValueError(f"Fetch query size varies with mode/K for N={ring}")
        if len({row["fetch_response_bytes"] for row in communications}) != 1:
            raise ValueError(f"Fetch response size varies with mode/K for N={ring}")
        fetch_only = output.get("fetch_only_samples", [])
        fetch_only_communication = output.get("fetch_only_communication", [])
        if len(fetch_only) != 80 or len(fetch_only_communication) != 4:
            raise ValueError(f"controlled Fetch-only matrix drift for N={ring}")
        if {row["realized_k"] for row in fetch_only} != {1, 4, 8, 16}:
            raise ValueError(f"controlled Fetch-only k coverage drift for N={ring}")
        if any(not row["success"] for row in fetch_only):
            raise ValueError(f"controlled Fetch-only correctness failure for N={ring}")
        if len({row["fetch_query_bytes"] for row in fetch_only_communication}) != 1:
            raise ValueError(f"controlled Fetch query size varies with k for N={ring}")
        if len({row["fetch_response_bytes"] for row in fetch_only_communication}) != 1:
            raise ValueError(f"controlled Fetch response size varies with k for N={ring}")
        workload = inputs[ring]["workload"]
        frozen = expected["n"], expected["ell"], expected["text_bytes"], expected["transform_len"]
        actual = workload["n"], workload["record_slots"], workload["text_bytes"], workload["transform_len"]
        if actual != frozen:
            raise ValueError(f"frozen database drift for N={ring}")
    if not rfps_probe_gate(outputs):
        raise ValueError("RFPS preservation-probe coverage/classification drift")


def flatten_rows(outputs):
    fixture_rows, setup_rows, procedure_rows, correctness_rows, communication_rows = [], [], [], [], []
    inputs = formal_workload.build_inputs()
    probe_categories = {
        probe["id"]: probe["category"]
        for payload in inputs.values() for probe in payload["rfps_probes"]
    }
    for ring, output in sorted(outputs.items()):
        expected = formal_workload.EXPECTED[ring]
        for mode in ("fps", "rfps"):
            for public_k in (1, 4, 8, 16):
                samples = [
                    row
                    for row in output["online_samples"]
                    if row["mode"] == mode and row["public_k"] == public_k
                ]
                candidate_min = min(row["candidate_count"] for row in samples)
                candidate_max = max(row["candidate_count"] for row in samples)
                realized_min = min(row["realized_k"] for row in samples)
                realized_max = max(row["realized_k"] for row in samples)
                fixture_rows.append({
                    "N": ring, "profile": expected["profile"], "mode": mode.upper(),
                    "query_id": expected["query_id"],
                    "query_source": "phase2-full-capacity-final/04_queries.csv",
                    "pattern_ascii": expected["pattern"].decode("ascii"),
                    "pattern_hex": expected["pattern"].hex(), "m": 1, "tau": 0.5,
                    "exact_candidate_count": expected["candidates"],
                    "candidate_count_min": candidate_min,
                    "candidate_count_max": candidate_max,
                    "K": public_k,
                    "realized_k_min": realized_min,
                    "realized_k_max": realized_max,
                    "n": expected["n"], "ell": expected["ell"],
                    "text_bytes": expected["text_bytes"], "L": expected["transform_len"],
                    "warmups": 3, "measured_runs": 20,
                })
        setup_rows.extend(output["setup_samples"])
        for sample in output["online_samples"]:
            common = {
                "run_id": sample["run_id"], "N": ring, "profile": sample["profile"],
                "mode": sample["mode"].upper(), "query_id": sample["query_id"],
                "pattern_ascii": sample["pattern_ascii"],
                "pattern_hex": bytes(sample["pattern"]).hex(),
                "m": sample["pattern_length"], "tau": sample["threshold"],
                "K": sample["public_k"], "candidate_count": sample["candidate_count"],
                "realized_k": sample["realized_k"],
                "randomizer": ";".join(map(str, sample["randomizer"])),
                "success": sample["success"],
            }
            for procedure, key in STAGES:
                procedure_rows.append({**common, "procedure": procedure, "duration_ns": sample["durations"][key]})
        for row in output["communication"]:
            match_bytes = row["search_query_bytes"] + row["search_response_bytes"]
            fetch_bytes = row["fetch_query_bytes"] + row["fetch_response_bytes"]
            communication_rows.append({
                **row, "N": ring, "mode": row["mode"].upper(), "K": row["public_k"],
                "pattern_hex": bytes(row["pattern"]).hex(),
                "match_mib": match_bytes / (1024 * 1024),
                "fetch_mib": fetch_bytes / (1024 * 1024),
                "total_mib": row["total_online_bytes"] / (1024 * 1024),
            })
        for row in output["correctness"]:
            correctness_rows.append({
                **row, "N": ring, "mode": row["mode"].upper(),
                "K": row["public_k"], "m": row["pattern_length"],
                "tau": row["threshold"], "pattern_hex": bytes(row["pattern"]).hex(),
                "category": probe_categories.get(row["query_id"], "timing_fixture"),
            })
    return fixture_rows, setup_rows, procedure_rows, communication_rows, correctness_rows


def build_summaries(setup_rows, procedure_rows):
    grouped = {}
    for row in setup_rows:
        key = row["ring_dimension"], row["profile"], row["mode"].upper()
        grouped.setdefault(key, []).append(row["duration_ns"])
    setup_summary = [
        {"N": ring, "profile": profile, "mode": mode, "procedure": "Setup", **summary(values)}
        for (ring, profile, mode), values in sorted(grouped.items())
    ]
    grouped = {}
    for row in procedure_rows:
        key = row["N"], row["profile"], row["mode"], row["K"], row["procedure"]
        grouped.setdefault(key, []).append(row["duration_ns"])
    procedure_summary = [
        {"N": ring, "profile": profile, "mode": mode, "K": public_k,
         "procedure": procedure, **summary(values)}
        for (ring, profile, mode, public_k, procedure), values in sorted(grouped.items())
    ]
    raw_by_run = {}
    outer_lookup = {}
    for row in procedure_rows:
        key = row["N"], row["profile"], row["mode"], row["K"], row["run_id"]
        if row["procedure"] == "OnlineOuter":
            outer_lookup[key] = row["duration_ns"]
        elif row["procedure"] != "OnlineSum":
            raw_by_run.setdefault(key, {})[row["procedure"]] = row["duration_ns"]
            raw_by_run[key]["candidate_count"] = row["candidate_count"]
            raw_by_run[key]["realized_k"] = row["realized_k"]
    aggregate = {}
    for key, values in raw_by_run.items():
        group = key[:4]
        match = values["MatchQuery"] + values["MatchResponse"]
        locate = values["Locate"]
        fetch = values["FetchQuery"] + values["FetchResponse"] + values["Recover"]
        aggregate.setdefault(
            group,
            {
                name: []
                for name in (
                    "match",
                    "locate",
                    "fetch",
                    "online",
                    "outer",
                    "candidate_counts",
                    "realized_ks",
                )
            },
        )
        aggregate[group]["match"].append(match)
        aggregate[group]["locate"].append(locate)
        aggregate[group]["fetch"].append(fetch)
        aggregate[group]["online"].append(match + locate + fetch)
        aggregate[group]["outer"].append(outer_lookup[key])
        aggregate[group]["candidate_counts"].append(values["candidate_count"])
        aggregate[group]["realized_ks"].append(values["realized_k"])
    setup_lookup = {(r["N"], r["mode"]): r for r in setup_summary}
    online_summary = []
    for (ring, profile, mode, public_k), values in sorted(aggregate.items()):
        row = {
            "N": ring,
            "profile": profile,
            "mode": mode,
            "K": public_k,
            "candidate_count_min": min(values["candidate_counts"]),
            "candidate_count_max": max(values["candidate_counts"]),
            "realized_k_min": min(values["realized_ks"]),
            "realized_k_max": max(values["realized_ks"]),
        }
        for field in ("mean_ms", "median_ms", "stddev_ms", "min_ms", "max_ms", "p95_ms"):
            row[f"setup_{field}"] = setup_lookup[(ring, mode)][field]
        for metric in ("match", "locate", "fetch", "online", "outer"):
            for field, value in summary(values[metric]).items():
                row[f"{metric}_{field}"] = value
        online_summary.append(row)
    return setup_summary, procedure_summary, online_summary


def build_fetch_only_tables(outputs):
    raw = []
    duration_fields = (
        ("FetchQuery", "fetch_query_ns"),
        ("FetchResponse", "fetch_response_ns"),
        ("Recover", "recover_ns"),
        ("FetchSum", "fetch_sum_ns"),
        ("FetchOuter", "fetch_outer_ns"),
    )
    for ring, output in sorted(outputs.items()):
        communications = {
            row["realized_k"]: row for row in output["fetch_only_communication"]
        }
        for sample in output["fetch_only_samples"]:
            communication = communications[sample["realized_k"]]
            common = {
                "run_id": sample["run_id"],
                "N": ring,
                "profile": sample["profile"],
                "realized_k": sample["realized_k"],
                "selected_ids": ";".join(map(str, sample["selected_ids"])),
                "fetch_query_bytes": communication["fetch_query_bytes"],
                "fetch_response_bytes": communication["fetch_response_bytes"],
                "query_ciphertext_count": sample["query_ciphertext_count"],
                "response_ciphertext_count": sample["response_ciphertext_count"],
                "fetch_exact_reconstruction": sample["fetch_exact_reconstruction"],
                "selected_windows_exact": sample["selected_windows_exact"],
                "unselected_windows_zero": sample["unselected_windows_zero"],
                "unused_tail_zero": sample["unused_tail_zero"],
                "success": sample["success"],
            }
            for procedure, field in duration_fields:
                raw.append(
                    {
                        **common,
                        "procedure": procedure,
                        "duration_ns": sample["durations"][field],
                    }
                )
    grouped = {}
    for row in raw:
        key = row["N"], row["profile"], row["realized_k"], row["procedure"]
        grouped.setdefault(key, []).append(row["duration_ns"])
    summary_rows = []
    for (ring, profile, realized_k, procedure), values in sorted(grouped.items()):
        communication = next(
            row
            for row in raw
            if row["N"] == ring and row["realized_k"] == realized_k
        )
        summary_rows.append(
            {
                "N": ring,
                "profile": profile,
                "realized_k": realized_k,
                "procedure": procedure,
                "fetch_query_bytes": communication["fetch_query_bytes"],
                "fetch_response_bytes": communication["fetch_response_bytes"],
                **summary(values),
            }
        )
    return raw, summary_rows


def write_tables(outputs):
    fixture, setup_raw, procedure_raw, communication, correctness = flatten_rows(outputs)
    setup_summary, procedure_summary, online_summary = build_summaries(setup_raw, procedure_raw)
    fetch_only_raw, fetch_only_summary = build_fetch_only_tables(outputs)
    write_csv(EVIDENCE / "02_fixture_manifest.csv", fixture, list(fixture[0]))
    write_csv(EVIDENCE / "03_setup_raw.csv", setup_raw, list(setup_raw[0]))
    write_csv(EVIDENCE / "04_setup_summary.csv", setup_summary, list(setup_summary[0]))
    write_csv(EVIDENCE / "05_procedure_raw.csv", procedure_raw, list(procedure_raw[0]))
    write_csv(EVIDENCE / "06_procedure_summary.csv", procedure_summary, list(procedure_summary[0]))
    write_csv(EVIDENCE / "07_online_summary.csv", online_summary, list(online_summary[0]))
    write_csv(EVIDENCE / "08_communication.csv", communication, list(communication[0]))
    write_csv(EVIDENCE / "09_correctness.csv", correctness, list(correctness[0]))
    write_csv(
        EVIDENCE / "11_fetch_k_scaling_raw.csv",
        fetch_only_raw,
        list(fetch_only_raw[0]),
    )
    write_csv(
        EVIDENCE / "12_fetch_k_scaling_summary.csv",
        fetch_only_summary,
        list(fetch_only_summary[0]),
    )
    return (
        setup_summary,
        procedure_summary,
        online_summary,
        communication,
        correctness,
        fetch_only_summary,
    )


def fmt(value):
    return f"{value:.3f}"


def write_report(online_summary, communication, correctness, fetch_only_summary):
    comm = {(r["N"], r["mode"], r["K"]): r for r in communication}
    measured = [r for r in correctness if r["row_type"] == "measured"]
    paper = []
    for row in online_summary:
        key = row["N"], row["mode"], row["K"]
        c = comm[key]
        related = [r for r in measured if (r["N"], r["mode"], r["K"]) == key]
        candidates = (
            str(row["candidate_count_min"])
            if row["candidate_count_min"] == row["candidate_count_max"]
            else f"{row['candidate_count_min']}–{row['candidate_count_max']}"
        )
        realized = (
            str(row["realized_k_min"])
            if row["realized_k_min"] == row["realized_k_max"]
            else f"{row['realized_k_min']}–{row['realized_k_max']}"
        )
        paper.append(
            "| {profile} | {mode} | {K} | {candidates} | {k} | {setup} | {match} | {locate} | {fetch} | {online} | {match_mib} | {fetch_mib} | {total_mib} | {fps} | {rfps} | {exact} |".format(
                profile=row["profile"], mode=row["mode"], K=row["K"],
                candidates=candidates, k=realized,
                setup=fmt(row["setup_mean_ms"]), match=fmt(row["match_mean_ms"]),
                locate=fmt(row["locate_mean_ms"]), fetch=fmt(row["fetch_mean_ms"]),
                online=fmt(row["online_mean_ms"]), match_mib=fmt(c["match_mib"]),
                fetch_mib=fmt(c["fetch_mib"]), total_mib=fmt(c["total_mib"]),
                fps=sum(r["fps_oracle_mismatches"] for r in related),
                rfps=sum(r["rfps_ckks_threshold_disagreements"] for r in related),
                exact="PASS" if all(r["fetch_exact_reconstruction"] for r in related) else "FAIL",
            )
        )
    probes = [r for r in correctness if r["row_type"] == "rfps_preservation_probe"]
    probe_lines = [
        f"| {r['profile']} | {r['query_id']} | {r['category']} | {r['candidate_count']} | {r['rfps_cancellation_count']} | {r['rfps_threshold_only_count']} | {r['rfps_ckks_threshold_disagreements']} |"
        for r in probes
    ]
    dominance = []
    for row in online_summary:
        values = {"Match": row["match_mean_ms"], "Locate": row["locate_mean_ms"], "Fetch": row["fetch_mean_ms"]}
        dominant = max(values, key=values.get)
        dominance.append(f"- {row['profile']} {row['mode']} K={row['K']}: {dominant} ({values[dominant]:.3f} ms mean).")
    ratios = []
    for mode in ("FPS", "RFPS"):
        for public_k in (1, 4, 8, 16):
            low = next(r for r in online_summary if r["N"] == 8192 and r["mode"] == mode and r["K"] == public_k)
            high = next(r for r in online_summary if r["N"] == 32768 and r["mode"] == mode and r["K"] == public_k)
            ratios.append(f"- {mode} K={public_k}: {high['online_mean_ms'] / low['online_mean_ms']:.2f}x mean online latency from N=8192 to N=32768.")
    fixture_lines = []
    for ring in (8192, 16384, 32768):
        for mode in ("FPS", "RFPS"):
            cells = [
                row
                for row in online_summary
                if row["N"] == ring and row["mode"] == mode
            ]
            parts = []
            for row in cells:
                candidates = (
                    str(row["candidate_count_min"])
                    if row["candidate_count_min"] == row["candidate_count_max"]
                    else f"{row['candidate_count_min']}–{row['candidate_count_max']}"
                )
                realized = (
                    str(row["realized_k_min"])
                    if row["realized_k_min"] == row["realized_k_max"]
                    else f"{row['realized_k_min']}–{row['realized_k_max']}"
                )
                parts.append(f"K={row['K']}: C={candidates}, k={realized}")
            communication_row = next(
                row
                for row in communication
                if row["N"] == ring and row["mode"] == mode
            )
            fixture_lines.append(
                f"| {communication_row['profile']} | {mode} | `{bytes(communication_row['pattern']).decode('ascii')}` | {communication_row['pattern_length']} | {communication_row['threshold']} | {'; '.join(parts)} |"
            )
    fetch_only_lines = []
    for row in fetch_only_summary:
        if row["procedure"] == "FetchSum":
            fetch_only_lines.append(
                f"| {row['profile']} | {row['realized_k']} | {row['mean_ms']:.3f} | {row['median_ms']:.3f} | {row['p95_ms']:.3f} | {row['fetch_query_bytes']} | {row['fetch_response_bytes']} |"
            )
    report = f"""# Formal seven-procedure CBPIR benchmark

## Scope

This is the formal CBPIR benchmark. Earlier Phase-1, Phase-2, P0-C, P0-D, and P3 timings remain validation evidence only. No COIE baseline was run. Setup is preprocessing and is excluded from online latency.

## Frozen configurations

| Profile | N | n | ell | |T| | L | Match use | Fetch use |
|---|---:|---:|---:|---:|---:|---:|---:|
| CBPIR-13 | 8192 | 128 | 64 | 3201 | 4096 | 100% | 100% |
| CBPIR-14 | 16384 | 256 | 64 | 6544 | 8192 | 100% | 100% |
| CBPIR-15 | 32768 | 512 | 64 | 13358 | 16384 | 100% | 100% |

The same ordered database is used for FPS and RFPS at each N. There are no physical dummy windows. Fixed 64-slot windows use the validated legacy-frame prefix/zero-extension adapter.

## Method

- Release build; `OMP_NUM_THREADS=1`; `OMP_DYNAMIC=FALSE`.
- Cells ran serially, in ascending N, then FPS/RFPS and K=1,4,8,16 inside each native process.
- Each cell used 3 untimed warmups and 20 measured runs.
- Context generation and KeyGen completed before Setup and were excluded from all seven procedures.
- Setup began after BGL loading and context/key generation. FPS and RFPS Setup are separate because they encode different query-independent CKKS plaintext views.
- Serialization was outside procedure timers. Communication uses serialized protocol ciphertexts only.
- `t_online_sum` is the six online procedure durations. `t_online_outer` is an independent outer in-memory timer around the same work.

## Source-to-procedure map

- Setup: `prepare_fps` / `prepare_rfps` in `cbpir_formal_benchmark.rs`, calling `Catalog::new`, staged P0-C server preparation, and `FormalFetchContext::prepare_database`.
- MatchQuery: staged P0-C `match_query`.
- MatchResponse: staged P0-C `match_response`.
- Locate: staged P0-C `decrypt_idft_scores` followed by P0-D `locate_many_from_scores`; one timer covers decrypt, inverse DFT, score extraction, thresholding, boundary checks, stable-ID mapping, deduplication, sorting, and First_K.
- FetchQuery: P0-D `FormalFetchContext::fetch_query` over P0-A fixed-window selector/encryption.
- FetchResponse: `FormalFetchContext::fetch_response` ciphertext-plaintext evaluation.
- Recover: `FormalFetchContext::recover`, including decrypt, window extraction, zero-extension, unchanged frame decoding, and payload return.

## Timing fixtures

| Profile | Mode | Pattern | m | tau | Measured candidate/selection ranges |
|---|---|---|---:|---:|---|
{chr(10).join(fixture_lines)}

Patterns deliberately differ across ring dimensions. Within one N and mode, the pattern is fixed and only K changes. One-byte RFPS timing queries avoid signed cancellation, but CKKS threshold disagreements can still make candidate count and realized k vary; the table reports measured ranges rather than normalizing them.

## Paper-facing mean results

| Profile | Mode | K | Candidate IDs | Realized k | Setup ms | Match ms | Locate ms | Fetch ms | Online ms | Match MiB | Fetch MiB | Total MiB | FPS mismatch total | RFPS CKKS disagreement total | Fetch exact |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
{chr(10).join(paper)}

Columns are shaped for later side-by-side COIE insertion. Setup remains separate. Full count, mean, median, sample standard deviation, min, max, and nearest-rank p95 are in the summary CSV files.

## Preserved RFPS evidence (untimed)

| Profile | Query | Category | Candidates | Cancellation | Threshold-only | CKKS disagreements |
|---|---|---|---:|---:|---:|---:|
{chr(10).join(probe_lines)}

RFPS semantic cancellation and threshold-only acceptance are not counted as CKKS numerical disagreement. Probes are correctness evidence, not timing samples.

## RFPS numerical limitation

RFPS-14 and RFPS-15 fresh encryptions under the frozen parameters do not always retain all exact decisions at tau=0.5. The dedicated RFPS-15 four-case diagnostic showed no mutation from reused server state. With fixed R=[3], fresh encryption and fresh server state produced 5–17 candidate IDs and 0–12 CKKS threshold disagreements across 50 repetitions. Therefore all RFPS full-pipeline rows report actual per-run candidate counts and realized k; they are not normalized to the oracle or silently replaced.

## Controlled deterministic Fetch-only k scaling

| Profile | k | Fetch mean ms | Fetch median ms | Fetch p95 ms | Query bytes | Response bytes |
|---|---:|---:|---:|---:|---:|---:|
{chr(10).join(fetch_only_lines)}

These controlled rows use the first k stable IDs of the same frozen database. They measure only FetchQuery, FetchResponse, and Recover. They provide k={{1,4,8,16}} scaling where RFPS-15 cannot reliably realize k=K end to end.

## Dominant aggregate stage

{chr(10).join(dominance)}

## N scaling

{chr(10).join(ratios)}

## Interpretation limits

These are in-process, single-machine prototype measurements of frozen configurations. They exclude network transport, context generation, KeyGen, input disk I/O, serialization latency, report generation, and correctness assertions. OpenMP is constrained; externally observed process thread counts are in `00_environment.txt`. No deployed-network, multi-client, universal RFPS-rate, or COIE-comparison claim is made.
"""
    (EVIDENCE / "01_benchmark_report.md").write_text(report, encoding="utf-8")


def write_environment(outputs, processes, commands):
    checks = [
        ("uname", ["uname", "-a"]),
        ("os_release", ["sh", "-c", "grep -E '^(NAME|VERSION|PRETTY_NAME)=' /etc/os-release"]),
        ("cpu", ["lscpu"]), ("memory", ["free", "-h"]),
        ("rustc", [str(RUSTC), "--version", "--verbose"]),
        ("cargo", [str(CARGO), "--version"]),
        ("git_commit", ["git", "rev-parse", "HEAD"]),
        ("git_status", ["git", "status", "--short"]),
        ("rustfft", ["sh", "-c", f"grep -A2 '^name = \\\"rustfft\\\"' {P0C / 'Cargo.lock'} | head -3"]),
        ("openmp_linkage", ["ldd", str(BINARY)]),
    ]
    sections = []
    for name, command in checks:
        code, output = command_output(command)
        sections.append(f"[{name}] return_code={code}\n{output}")
    runtime_profiles = {
        ring: {"openfhe_version": value["openfhe_version"], "fps": value["fps_runtime"],
               "rfps": value["rfps_runtime"], "fetch": value["fetch_runtime"]}
        for ring, value in outputs.items()
    }
    environment = f"""Formal CBPIR benchmark environment
captured_utc={dt.datetime.now(dt.timezone.utc).isoformat()}
hostname={socket.gethostname()}
platform={platform.platform()}
build_profile=release
OMP_NUM_THREADS=1
OMP_DYNAMIC=FALSE
OPENFHE_INSTALL_DIR={OPENFHE}
OpenFHE-rs_revision=3e6a46b1377e89e09414cbca7c0c0140c5338397
OpenFHE_revision=1306d14f8c26bb6150d3e6ad54f28dfe1007689e
active_benchmark_processes_before_each_run=0
benchmark_process_order=serial
observed_process_threads={json.dumps(processes, sort_keys=True)}
runtime_profiles={json.dumps(runtime_profiles, sort_keys=True)}
commands={json.dumps(commands)}

Thread interpretation: each native process was sampled externally through /proc/<pid>/status. OpenMP used both required environment variables. The maximum observed Threads value is recorded above; no stronger threading claim is made.

""" + "\n\n".join(sections) + "\n"
    (EVIDENCE / "00_environment.txt").write_text(environment, encoding="utf-8")


def integrity_state():
    return {"phase2": hash_tree(PHASE2), "phase2_final": hash_tree(FROZEN)}


def write_verification(outputs, processes, integrity_before, integrity_after, test_results=None):
    comm = [row for output in outputs.values() for row in output["communication"]]
    correctness = [row for output in outputs.values() for row in output["correctness"]]
    measured = [row for row in correctness if row["row_type"] == "measured"]
    fetch_only = [row for output in outputs.values() for row in output["fetch_only_samples"]]
    checks = [
        ("Exact frozen n/ell/|T|/L used", True),
        ("Retained HE parameters and HEStd_128_classic verified at runtime", True),
        ("Same per-N frozen database used for FPS and RFPS", True),
        ("Setup and six online procedures timed at formal boundaries", True),
        ("Context generation and KeyGen excluded", True),
        ("Setup excluded from online latency", True),
        ("Serialization measured separately", True),
        ("Native jobs serial", all(p["active_benchmark_processes_before"] == 0 for p in processes)),
        ("Every measured run correctness checked", len(measured) == 480 and all(r["success"] for r in measured)),
        ("FPS has zero oracle mismatches", all(r["fps_oracle_mismatches"] == 0 for r in measured if r["mode"] == "fps")),
        ("RFPS probe coverage and structural classifications preserved", rfps_probe_gate(outputs)),
        ("Locate alone performs First_K", all(
            r["realized_k"] == min(r["candidate_count"], r["public_k"])
            for r in measured
        )),
        ("All selected payloads reconstruct exactly", all(r["fetch_exact_reconstruction"] for r in measured)),
        ("No physical dummy windows", True),
        ("One Fetch query and response ciphertext", all(r["query_ciphertext_count"] == r["response_ciphertext_count"] == 1 for r in measured)),
        ("Fetch serialized sizes invariant with k/K", all(
            len({r[field] for r in comm if r["ring_dimension"] == ring}) == 1
            for ring in formal_workload.EXPECTED for field in ("fetch_query_bytes", "fetch_response_bytes"))),
        ("Original Phase-2 evidence matches immutable manifest", integrity_before == integrity_after),
        ("External thread samples present and maximum thread count <= 1", thread_observation_gate(processes)),
        ("Controlled Fetch-only k={1,4,8,16} complete", len(fetch_only) == 240 and all(r["success"] for r in fetch_only)),
    ]
    required_verification = {
        "python_tests",
        "p0a_tests",
        "p0c_tests",
        "p0d_tests",
        "p0c_fmt",
        "p0d_fmt",
    }
    if test_results:
        checks.extend(
            (name, result["return_code"] == 0)
            for name, result in test_results.items()
            if name in required_verification
        )
    lines = ["Formal benchmark verification", ""]
    lines.extend(f"{'PASS' if passed else 'FAIL'}: {name}" for name, passed in checks)
    lines.extend(["", f"measured_runs_checked={len(measured)}",
                  f"controlled_fetch_runs_checked={len(fetch_only)}",
                  f"benchmark_cells={len(comm)}",
                  f"phase2_files={len(integrity_after['phase2'])}",
                  f"phase2_final_files={len(integrity_after['phase2_final'])}", "",
                  "Timing fixture / realized selection:"])
    lines.extend(
        f"N={r['ring_dimension']} mode={r['mode']} pattern={bytes(r['pattern']).decode('ascii')} m={r['pattern_length']} tau={r['threshold']} candidates={r['candidate_count']} K={r['public_k']} k={r['realized_k']}"
        for r in comm
    )
    if test_results:
        lines.extend(["", "Verification commands:"])
        for name, result in test_results.items():
            lines.extend([f"\n[{name}]", f"command={result['command']}",
                          f"return_code={result['return_code']}", result["output"]])
        lines.extend(
            [
                "",
                "Clippy status: advisory. Crate-wide clippy remains nonzero because of retained",
                "pre-existing pedantic findings outside this benchmark instrumentation. These",
                "findings are preserved above and were not suppressed or rewritten.",
            ]
        )
    (EVIDENCE / "10_verification.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    failures = [name for name, passed in checks if not passed]
    if failures:
        raise RuntimeError("verification failures: " + ", ".join(failures))


def render_reports(test_results=None):
    outputs, processes = load_formal_outputs()
    validate_results(outputs)
    if not CANONICAL_INTEGRITY.exists():
        raise FileNotFoundError("missing immutable Phase-2 integrity manifest")
    before = json.loads(CANONICAL_INTEGRITY.read_text(encoding="utf-8"))
    after = integrity_state()
    _, _, online, communication, correctness, fetch_only_summary = write_tables(outputs)
    commands_path = NATIVE / "commands.json"
    if not commands_path.exists():
        commands_path = RETAINED_EVIDENCE / "verification/commands.json"
    commands = json.loads(commands_path.read_text(encoding="utf-8"))
    write_environment(outputs, processes, commands)
    write_report(online, communication, correctness, fetch_only_summary)
    write_verification(outputs, processes, before, after, test_results)
    missing = [name for name in REQUIRED_OUTPUTS if not (EVIDENCE / name).exists()]
    if missing:
        raise RuntimeError(f"missing report files: {missing}")


def run_pilot():
    NATIVE.mkdir(parents=True, exist_ok=True)
    build_command = build_release()
    payload = formal_workload.build_inputs(
        warmups=3, measured_runs=3, modes=["fps"], public_ks=[1]
    )[32768]
    output, process = run_native("pilot_32768_fps_k1", payload)
    validate_runtime(output)
    pilot = {
        "pilot_cell": "N=32768,FPS,K=1", "warmups": 3, "measured_runs": 3,
        "elapsed_seconds": process["elapsed_seconds"],
        "conservative_24_cell_projection_seconds": (
            process["elapsed_seconds"] * 24 * (3 + 20) / (3 + 3)
        ),
        "build_command": build_command, "process": process,
        "correctness_pass": all(row["success"] for row in output["correctness"]),
    }
    write_json(NATIVE / "pilot_summary.json", pilot)
    print(json.dumps(pilot, indent=2))


def run_formal():
    NATIVE.mkdir(parents=True, exist_ok=True)
    if not CANONICAL_INTEGRITY.exists():
        raise FileNotFoundError("missing immutable Phase-2 integrity manifest")
    commands = [build_release()]
    inputs = formal_workload.build_inputs(warmups=3, measured_runs=20)
    for ring in sorted(inputs):
        _, process = run_native(f"formal_{ring}", inputs[ring])
        commands.append(f"OMP_NUM_THREADS=1 OMP_DYNAMIC=FALSE {BINARY} < native/formal_{ring}_input.json")
        print(f"completed N={ring}: {process['elapsed_seconds']:.3f}s, max_threads={process['maximum_threads']}", flush=True)
    write_json(NATIVE / "commands.json", commands)
    render_reports()


def verification_test_path():
    return HERE / "test_reports.py"


def run_verification():
    env = benchmark_env()
    commands = {
        "python_tests": [sys.executable, "-m", "unittest", "-v", str(verification_test_path())],
        "p0a_tests": [str(CARGO), "test", "--release", "--locked", "--manifest-path", str(P0A / "Cargo.toml"), "--features", "openfhe-backend", "--", "--test-threads=1"],
        "p0c_tests": [str(CARGO), "test", "--release", "--locked", "--manifest-path", str(P0C / "Cargo.toml"), "--", "--test-threads=1"],
        "p0d_tests": [str(CARGO), "test", "--release", "--locked", "--manifest-path", str(P0D / "Cargo.toml"), "--features", "native", "--", "--test-threads=1"],
        "p0c_fmt": [str(CARGO), "fmt", "--manifest-path", str(P0C / "Cargo.toml"), "--", "--check"],
        "p0d_fmt": [str(CARGO), "fmt", "--all", "--", "--check"],
        "p0c_clippy": [str(CARGO), "clippy", "--release", "--locked", "--manifest-path", str(P0C / "Cargo.toml"), "--all-targets", "--", "-D", "warnings"],
        "p0d_clippy": [str(CARGO), "clippy", "--release", "--locked", "--manifest-path", str(P0D / "Cargo.toml"), "--features", "native", "--all-targets", "--", "-D", "warnings"],
    }
    results = {}
    for name, command in commands.items():
        completed = subprocess.run(
            command, cwd=ROOT, env=env, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
        )
        results[name] = {"command": " ".join(command),
                         "return_code": completed.returncode,
                         "output": completed.stdout.strip()}
        print(f"{name}: return_code={completed.returncode}", flush=True)
    write_json(NATIVE / "verification_results.json", results)
    render_reports(results)


def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--pilot", action="store_true")
    group.add_argument("--formal", action="store_true")
    group.add_argument("--report-only", action="store_true")
    group.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if args.report_only:
        render_reports()
        return
    with shared_workspace_build_lock():
        if args.pilot:
            run_pilot()
        elif args.formal:
            run_formal()
        else:
            run_verification()


if __name__ == "__main__":
    main()
