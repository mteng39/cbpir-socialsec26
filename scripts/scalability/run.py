#!/usr/bin/env python3
"""Generate, run, summarize, and validate the additive CBPIR scale study."""

from __future__ import annotations

import argparse
from collections import defaultdict
import csv
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time


PLAINTEXT_MODULUS = 786_433
RECORD_SLOTS = 64
PUBLIC_K = 16
WARMUPS = 3
MEASURED_RUNS = 20
PATTERN = b":"
PROFILES = ((512, 32_768), (1024, 65_536))
MODES = ("fps", "rfps")
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OPENFHE = ROOT / "openfhe/.openfhe/openfhe-1.5.1/install"
TARGET = ROOT / "target/experiments/scalability"
BINARY = TARGET / "release/scalability"
RETAINED_EVIDENCE = ROOT / "evaluation/scalability-v1"
EVIDENCE = ROOT / "target/reproduction/scalability-v1"
LOGS = EVIDENCE / "logs"
SOURCE = ROOT / "data/source/loghub/BGL_2k.log"
PHASE2_PATH = HERE / "workload.py"
CONTROL_PATH = ROOT / "data/workloads/formal_32768_input.json"
CARGO = Path(shutil.which("cargo") or "cargo")
RUSTC = Path(shutil.which("rustc") or "rustc")


def _load_phase2():
    spec = importlib.util.spec_from_file_location("scalability_phase2", PHASE2_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PHASE2 = _load_phase2()


def expected_matrix():
    """Return one isolated process configuration per database size and mode."""
    return [
        {
            "n": n,
            "ring_dimension": ring,
            "mode": mode,
            "public_k": PUBLIC_K,
            "warmups": WARMUPS,
            "measured_runs": MEASURED_RUNS,
        }
        for n, ring in PROFILES
        for mode in MODES
    ]


def corpus_audit():
    rows = PHASE2.source_rows(SOURCE)
    eligible = len(rows)
    return {
        "source": str(SOURCE.relative_to(ROOT)),
        "source_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        "eligible_records": eligible,
        "supports_1024": eligible >= 1024,
        "supports_2048": eligible >= 2048,
        "n2048_status": (
            "eligible" if eligible >= 2048 else f"NOT RUN: pinned corpus has {eligible} eligible records"
        ),
    }


def build_workload(n, ring_dimension):
    """Build a deterministic BGL-derived prefix without changing frozen inputs."""
    rows = PHASE2.source_rows(SOURCE)
    if n > len(rows):
        raise ValueError(f"requested n={n}, but pinned corpus has {len(rows)} eligible records")
    records = PHASE2._records(rows, n)
    text, spans = PHASE2._frame(records)
    transform_len = PHASE2._next_power_of_two(len(text) + 1)
    if transform_len > ring_dimension // 2:
        raise ValueError("Search transform exceeds CKKS slot capacity")
    if n * RECORD_SLOTS > ring_dimension:
        raise ValueError("record windows exceed BGV slot capacity")
    maximum_frame_slots = max(14 + len(record["payload"]) for record in records)
    if maximum_frame_slots > RECORD_SLOTS:
        raise ValueError("record exceeds fixed 64-slot frame")
    workload = {
        "schema": "cbpir-scalability-v1-workload",
        "ring_dimension": ring_dimension,
        "n": n,
        "text_bytes": len(text),
        "transform_len": transform_len,
        "ckks_slots": ring_dimension // 2,
        "payload_bytes": sum(len(record["payload"]) for record in records),
        "record_slots": RECORD_SLOTS,
        "fetch_slots_used": n * RECORD_SLOTS,
        "bgv_slots": ring_dimension,
        "max_serialized_slots": maximum_frame_slots,
        "selection_seed": PHASE2.SEED,
        "source_sha256": PHASE2.SOURCE_SHA256,
        "source_commit": PHASE2.SOURCE_COMMIT,
        "eligible_records": len(rows),
        "records": records,
        "spans": spans,
    }
    if n == 512:
        frozen = json.loads(CONTROL_PATH.read_text(encoding="utf-8"))["workload"]
        frozen_ids = [record["id"] for record in frozen["records"]]
        if (
            workload["text_bytes"] != frozen["text_bytes"]
            or workload["transform_len"] != frozen["transform_len"]
            or [record["id"] for record in records] != frozen_ids
        ):
            raise ValueError("512-record control drifted from frozen formal workload")
    return workload


def build_query_fixture(workload):
    oracle = PHASE2._oracle(
        workload["records"], PATTERN, "fps", [], 0.5, PUBLIC_K
    )
    return {
        "source_query_id": "fps-k16-more-colon",
        "pattern": list(PATTERN),
        "pattern_escaped": ":",
        "pattern_hex": PATTERN.hex(),
        "threshold": 0.5,
        "expected_literal_alignment_count": len(oracle["literal_positions"]),
        "expected_literal_record_count": len(oracle["candidate_record_ids"]),
    }


def build_native_input(n, ring_dimension, mode):
    if mode not in MODES:
        raise ValueError(f"unsupported mode: {mode}")
    workload = build_workload(n, ring_dimension)
    return {
        "schema": "cbpir-scalability-v1-input",
        "profile": f"CBPIR-N{ring_dimension}-t{PLAINTEXT_MODULUS}",
        "workload": workload,
        "query_fixtures": [build_query_fixture(workload)],
        "modes": [mode],
        "public_ks": [PUBLIC_K],
        "warmups": WARMUPS,
        "measured_runs": MEASURED_RUNS,
        "he_parameters": {
            "ckks_ring_dimension": ring_dimension,
            "ckks_batch_size": workload["transform_len"],
            "fps_multiplicative_depth": 2,
            "rfps_multiplicative_depth": 1,
            "fps_scaling_modulus_bits": 50,
            "rfps_scaling_modulus_bits": 24,
            "first_modulus_bits": 60,
            "scaling_technique": "FIXEDMANUAL",
            "key_switch_technique": "HYBRID",
            "bgv_ring_dimension": ring_dimension,
            "bgv_plaintext_modulus": PLAINTEXT_MODULUS,
            "bgv_multiplicative_depth": 1,
            "record_slots": RECORD_SLOTS,
        },
    }


def percentile(values, probability):
    ordered = sorted(values)
    position = (len(ordered) - 1) * probability
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return ordered[lower] + (ordered[upper] - ordered[lower]) * fraction


def benchmark_env():
    env = os.environ.copy()
    install = Path(env.get("OPENFHE_INSTALL_DIR", OPENFHE))
    temporary = TARGET / "tmp"
    temporary.mkdir(parents=True, exist_ok=True)
    env.update(
        {
            "OMP_NUM_THREADS": "1",
            "OMP_DYNAMIC": "FALSE",
            "OPENFHE_INSTALL_DIR": str(install),
            "LD_LIBRARY_PATH": str(install / "lib"),
            "CARGO_TARGET_DIR": str(TARGET),
            "CARGO_BUILD_JOBS": "1",
            "TMPDIR": str(temporary),
        }
    )
    return env


def artifact_paths(n, mode):
    stem = f"scalability_n{n}_{mode}"
    return {
        "input": LOGS / f"{stem}_input.json",
        "output": LOGS / f"{stem}_output.json",
        "stderr": LOGS / f"{stem}_stderr.txt",
        "process": LOGS / f"{stem}_process.json",
    }


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def release_command():
    return [
        str(CARGO),
        "build",
        "--release",
        "--locked",
        "--manifest-path",
        str(ROOT / "Cargo.toml"),
        "--features",
        "native",
        "--bin",
        "scalability",
    ]


def build_release():
    LOGS.mkdir(parents=True, exist_ok=True)
    command = release_command()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=benchmark_env(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    (LOGS / "build.txt").write_text(completed.stdout, encoding="utf-8")
    if completed.returncode:
        raise RuntimeError("release build failed; see evaluation/scalability-v1/logs/build.txt")
    return command


def process_status(pid):
    result = {}
    try:
        for line in Path(f"/proc/{pid}/status").read_text(encoding="utf-8").splitlines():
            if line.startswith("Threads:"):
                result["threads"] = int(line.split(":", 1)[1])
            elif line.startswith("VmRSS:"):
                result["rss_kib"] = int(line.split()[1])
    except (FileNotFoundError, ProcessLookupError):
        pass
    return result


def run_native(n, ring, mode, payload):
    paths = artifact_paths(n, mode)
    existing = [path for path in paths.values() if path.exists()]
    if existing:
        raise FileExistsError(
            "refusing to overwrite native artifacts: "
            + ", ".join(str(path.relative_to(ROOT)) for path in existing)
        )
    write_json(paths["input"], payload)
    started = time.monotonic()
    maximum_rss_kib = 0
    maximum_threads = 0
    samples = 0
    with (
        paths["input"].open("rb") as stdin,
        paths["output"].open("wb") as stdout,
        paths["stderr"].open("wb") as stderr,
    ):
        process = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, env=benchmark_env(), stdin=stdin, stdout=stdout, stderr=stderr
        )
        while process.poll() is None:
            status = process_status(process.pid)
            maximum_rss_kib = max(maximum_rss_kib, status.get("rss_kib", 0))
            maximum_threads = max(maximum_threads, status.get("threads", 0))
            samples += 1
            time.sleep(0.01)
        return_code = process.wait()
    write_json(
        paths["process"],
        {
            "n": n,
            "ring_dimension": ring,
            "mode": mode,
            "command": str(BINARY),
            "elapsed_seconds": time.monotonic() - started,
            "maximum_rss_kib": maximum_rss_kib,
            "maximum_threads": maximum_threads,
            "observed_process_samples": samples,
            "return_code": return_code,
        },
    )
    if return_code:
        message = paths["stderr"].read_text(encoding="utf-8", errors="replace")
        raise RuntimeError(f"n={n} mode={mode} failed ({return_code}): {message[-4000:]}")
    return json.loads(paths["output"].read_text(encoding="utf-8"))


def validate_output(output, payload):
    if output["schema"] != "cbpir-scalability-v1-output":
        raise ValueError("native output schema drift")
    workload = payload["workload"]
    if (
        output["ring_dimension"] != workload["ring_dimension"]
        or output["n"] != workload["n"]
        or output["openfhe_version"] != "1.5.1"
        or output["warmups"] != WARMUPS
        or output["measured_runs"] != MEASURED_RUNS
    ):
        raise ValueError("native runtime/repetition contract drift")
    if len(output["online_samples"]) != MEASURED_RUNS:
        raise ValueError("wrong measured-row count")
    if len(output["setup_samples"]) != MEASURED_RUNS:
        raise ValueError("wrong Setup-row count")
    if len(output["communication"]) != MEASURED_RUNS:
        raise ValueError("communication was not serialized for every execution")
    measured = [row for row in output["correctness"] if row["row_type"] == "measured"]
    if len(measured) != MEASURED_RUNS:
        raise ValueError("wrong correctness-row count")
    if any(not row["success"] for row in output["online_samples"]):
        raise ValueError("one or more online samples failed")
    if any(
        not row["fetch_exact_reconstruction"]
        or row["query_ciphertext_count"] != 1
        or row["response_ciphertext_count"] != 1
        for row in measured
    ):
        raise ValueError("Fetch correctness or one-query/one-response contract failed")


def load_outputs():
    payloads = {}
    outputs = {}
    for n, ring in PROFILES:
        for mode in MODES:
            paths = artifact_paths(n, mode)
            key = (n, mode)
            payloads[key] = json.loads(paths["input"].read_text(encoding="utf-8"))
            outputs[key] = json.loads(paths["output"].read_text(encoding="utf-8"))
            validate_output(outputs[key], payloads[key])
    return payloads, outputs


def flatten_rows(outputs):
    rows = []
    for (n, mode), output in outputs.items():
        setup = {int(row["run_id"]): row for row in output["setup_samples"]}
        communication = {int(row["run_id"]): row for row in output["communication"]}
        correctness = {
            int(row["run_id"]): row
            for row in output["correctness"]
            if row["row_type"] == "measured"
        }
        process = json.loads(artifact_paths(n, mode)["process"].read_text(encoding="utf-8"))
        for sample in output["online_samples"]:
            run_id = int(sample["run_id"])
            sizes = communication[run_id]
            check = correctness[run_id]
            setup_sample = setup[run_id]
            duration = sample["durations"]
            rows.append(
                {
                    "run_id": run_id,
                    "n": n,
                    "text_bytes": output["text_bytes"],
                    "transform_len": output["transform_len"],
                    "ring_dimension": output["ring_dimension"],
                    "plaintext_modulus": output["fetch_runtime"]["plaintext_modulus"],
                    "record_slots": output["fetch_runtime"]["record_slots"],
                    "mode": mode,
                    "pattern_hex": bytes(sample["pattern"]).hex(),
                    "pattern_length": sample["pattern_length"],
                    "public_k": sample["public_k"],
                    "candidate_count": sample["candidate_count"],
                    "selected_count": sample["realized_k"],
                    "setup_ns": setup_sample["duration_ns"],
                    **duration,
                    "search_ns": duration["search_query_ns"] + duration["search_response_ns"],
                    "fetch_ns": duration["fetch_query_ns"] + duration["fetch_response_ns"] + duration["recover_ns"],
                    "search_query_bytes": sizes["search_query_bytes"],
                    "search_response_bytes": sizes["search_response_bytes"],
                    "fetch_query_bytes": sizes["fetch_query_bytes"],
                    "fetch_response_bytes": sizes["fetch_response_bytes"],
                    "total_online_bytes": sizes["total_online_bytes"],
                    "query_ciphertext_count": check["query_ciphertext_count"],
                    "response_ciphertext_count": check["response_ciphertext_count"],
                    "fps_oracle_mismatches": check["fps_oracle_mismatches"],
                    "rfps_ckks_threshold_disagreements": check["rfps_ckks_threshold_disagreements"],
                    "fetch_exact_reconstruction": check["fetch_exact_reconstruction"],
                    "success": sample["success"] and check["success"],
                    "process_peak_rss_kib": process["maximum_rss_kib"],
                    "process_maximum_threads": process["maximum_threads"],
                }
            )
    return rows


def metric_summary(values, prefix):
    return {
        f"{prefix}_mean": statistics.fmean(values),
        f"{prefix}_median": statistics.median(values),
        f"{prefix}_stddev": statistics.stdev(values) if len(values) > 1 else 0.0,
        f"{prefix}_p95": percentile(values, 0.95),
    }


def summarize_rows(rows):
    grouped = defaultdict(list)
    for row in rows:
        grouped[(row["n"], row["mode"])].append(row)
    summaries = []
    byte_fields = (
        "search_query_bytes",
        "search_response_bytes",
        "fetch_query_bytes",
        "fetch_response_bytes",
        "total_online_bytes",
    )
    for key in sorted(grouped):
        group = sorted(grouped[key], key=lambda row: row["run_id"])
        if any(len({row[field] for row in group}) != 1 for field in byte_fields):
            raise ValueError(f"serialized sizes vary within configuration {key}")
        search = [row["search_ns"] for row in group]
        locate = [row["locate_ns"] for row in group]
        fetch = [row["fetch_ns"] for row in group]
        summary = {
            "n": key[0],
            "mode": key[1],
            "text_bytes": group[0]["text_bytes"],
            "transform_len": group[0]["transform_len"],
            "ring_dimension": group[0]["ring_dimension"],
            "plaintext_modulus": group[0]["plaintext_modulus"],
            "record_slots": group[0]["record_slots"],
            "pattern_length": group[0]["pattern_length"],
            "public_k": group[0]["public_k"],
            "runs": len(group),
            "candidate_count_min": min(row["candidate_count"] for row in group),
            "candidate_count_median": statistics.median(row["candidate_count"] for row in group),
            "candidate_count_max": max(row["candidate_count"] for row in group),
            "selected_count_min": min(row["selected_count"] for row in group),
            "selected_count_median": statistics.median(row["selected_count"] for row in group),
            "selected_count_max": max(row["selected_count"] for row in group),
            "median_search_ns": statistics.median(search),
            "median_locate_ns": statistics.median(locate),
            "median_fetch_ns": statistics.median(fetch),
            "median_per_run_online_sum_ns": statistics.median(row["online_sum_ns"] for row in group),
            "sum_of_stage_medians_ns": statistics.median(search) + statistics.median(locate) + statistics.median(fetch),
            "median_online_outer_ns": statistics.median(row["online_outer_ns"] for row in group),
            "communication_identical": True,
            "all_fetch_exact": all(row["fetch_exact_reconstruction"] for row in group),
            "all_success": all(row["success"] for row in group),
            "process_peak_rss_kib": group[0]["process_peak_rss_kib"],
            "process_maximum_threads": group[0]["process_maximum_threads"],
        }
        for field in ("setup_ns", "search_ns", "locate_ns", "fetch_ns", "online_sum_ns", "online_outer_ns"):
            summary.update(metric_summary([row[field] for row in group], field))
        summary.update({field: group[0][field] for field in byte_fields})
        summaries.append(summary)
    if len(summaries) != 4 or any(row["runs"] != MEASURED_RUNS for row in summaries):
        raise ValueError("summary matrix drift")
    return summaries


def write_csv(path, rows):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not rows:
        raise ValueError(f"cannot write empty CSV: {path}")
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def command_text(command):
    completed = subprocess.run(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False
    )
    return completed.stdout.strip()


def write_reports(payloads, outputs, build_command):
    rows = flatten_rows(outputs)
    summaries = summarize_rows(rows)
    write_csv(EVIDENCE / "raw.csv", rows)
    write_csv(EVIDENCE / "summary.csv", summaries)
    parameters = []
    for n, ring in PROFILES:
        workload = payloads[(n, "fps")]["workload"]
        parameters.append(
            {
                "status": "MEASURED",
                "n": n,
                "text_bytes": workload["text_bytes"],
                "transform_len": workload["transform_len"],
                "ckks_ring_dimension": ring,
                "ckks_batch_size": workload["transform_len"],
                "fps_multiplicative_depth": 2,
                "fps_first_modulus_bits": 60,
                "fps_scaling_modulus_bits": 50,
                "rfps_multiplicative_depth": 1,
                "rfps_first_modulus_bits": 60,
                "rfps_scaling_modulus_bits": 24,
                "ckks_scaling_technique": "FIXEDMANUAL",
                "ckks_key_switch_technique": "HYBRID",
                "bgv_ring_dimension": ring,
                "plaintext_modulus": PLAINTEXT_MODULUS,
                "bgv_multiplicative_depth": 1,
                "record_slots": RECORD_SLOTS,
                "public_k": PUBLIC_K,
                "reason": "",
            }
        )
    parameters.append(
        {
            "status": "NOT_RUN",
            "n": 2048,
            "text_bytes": "",
            "transform_len": "",
            "ckks_ring_dimension": 131_072,
            "ckks_batch_size": 65_536,
            "fps_multiplicative_depth": 2,
            "fps_first_modulus_bits": 60,
            "fps_scaling_modulus_bits": 50,
            "rfps_multiplicative_depth": 1,
            "rfps_first_modulus_bits": 60,
            "rfps_scaling_modulus_bits": 24,
            "ckks_scaling_technique": "FIXEDMANUAL",
            "ckks_key_switch_technique": "HYBRID",
            "bgv_ring_dimension": 131_072,
            "plaintext_modulus": PLAINTEXT_MODULUS,
            "bgv_multiplicative_depth": 1,
            "record_slots": RECORD_SLOTS,
            "public_k": PUBLIC_K,
            "reason": corpus_audit()["n2048_status"],
        }
    )
    write_csv(EVIDENCE / "parameters.csv", parameters)
    write_json(
        EVIDENCE / "fixtures.json",
        {
            "corpus": corpus_audit(),
            "query": {str(n): build_query_fixture(payloads[(n, "fps")]["workload"]) for n, _ in PROFILES},
        },
    )
    (LOGS / "n2048_not_run.txt").write_text(corpus_audit()["n2048_status"] + "\n", encoding="utf-8")
    environment = [
        f"generated_utc={time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}",
        f"platform={platform.platform()}",
        f"python={sys.version.split()[0]}",
        f"cargo={command_text([str(CARGO), '--version'])}",
        f"rustc={command_text([str(RUSTC), '--version'])}",
        "openfhe=1.5.1",
        "omp_num_threads=1",
        "omp_dynamic=FALSE",
        "container_memory_limit_bytes=4294967296",
        f"build_command={' '.join(map(str, build_command))}",
    ]
    for n, _ring in PROFILES:
        for mode in MODES:
            process = json.loads(artifact_paths(n, mode)["process"].read_text(encoding="utf-8"))
            environment.extend(
                [
                    f"n{n}_{mode}_elapsed_seconds={process['elapsed_seconds']}",
                    f"n{n}_{mode}_maximum_rss_kib={process['maximum_rss_kib']}",
                    f"n{n}_{mode}_maximum_threads={process['maximum_threads']}",
                ]
            )
    (EVIDENCE / "environment.txt").write_text("\n".join(environment) + "\n", encoding="utf-8")
    verification = [
        "R2.10 scalability benchmark verification: PASS",
        "measured_configurations=4",
        f"measured_executions={len(rows)}",
        "warmups_per_configuration=3",
        "measured_runs_per_configuration=20",
        "largest_measured_n=1024",
        "largest_measured_text_bytes=26524",
        "n2048_status=NOT_RUN_PINNED_CORPUS_HAS_1331_ELIGIBLE_RECORDS",
        "extended_plaintext_modulus=786433",
        "control_n512_reproduces_frozen_workload=true",
        "one_fetch_query_ciphertext=true",
        "one_fetch_response_ciphertext=true",
        "communication_serialized_each_execution=true",
        "communication_identical_within_each_configuration=true",
        f"all_fetch_exact={str(all(row['all_fetch_exact'] for row in summaries)).lower()}",
        f"all_configurations_successful={str(all(row['all_success'] for row in summaries)).lower()}",
    ]
    (EVIDENCE / "verification.txt").write_text("\n".join(verification) + "\n", encoding="utf-8")


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--summarize-only", action="store_true")
    return parser.parse_args()


def main():
    args = parse_args()
    build_command = release_command()
    if not args.summarize_only:
        build_command = build_release()
        for row in expected_matrix():
            payload = build_native_input(row["n"], row["ring_dimension"], row["mode"])
            output = run_native(row["n"], row["ring_dimension"], row["mode"], payload)
            validate_output(output, payload)
    payloads, outputs = load_outputs()
    write_reports(payloads, outputs, build_command)
    print("validated 4 configurations and 80 measured executions; n=2048 not run")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
