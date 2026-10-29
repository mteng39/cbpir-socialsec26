#!/usr/bin/env python3
"""Generate, run, summarize, and validate the CBPIR pattern-length benchmark."""

from __future__ import annotations

import argparse
from collections import defaultdict
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time


RINGS = (8192, 16384, 32768)
PATTERN_LENGTHS = (1, 4, 8, 16)
MODES = ("fps", "rfps")
PUBLIC_K = 16
WARMUPS = 3
MEASURED_RUNS = 20
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OPENFHE = ROOT / "openfhe/.openfhe/openfhe-1.5.1/install"
TARGET = ROOT / "target/experiments/pattern-length"
BINARY = TARGET / "release/pattern_length"
RETAINED_EVIDENCE = ROOT / "evaluation/pattern-length-v1"
EVIDENCE = ROOT / "target/reproduction/pattern-length-v1"
LOGS = EVIDENCE / "logs"
CARGO = Path(shutil.which("cargo") or "cargo")
RUSTC = Path(shutil.which("rustc") or "rustc")


def expected_matrix():
    """Return the fixed 24-configuration R1.4 matrix."""
    return [
        {
            "ring_dimension": ring,
            "n": ring // 64,
            "pattern_length": pattern_length,
            "mode": mode,
            "public_k": PUBLIC_K,
        }
        for ring in RINGS
        for pattern_length in PATTERN_LENGTHS
        for mode in MODES
    ]


def enumerate_natural_patterns(records, pattern_length):
    """Enumerate in-record byte substrings and their record/alignment support."""
    candidates = {}
    global_start = 0
    for record_index, record in enumerate(records):
        searchable = bytes(record["searchable"])
        for local_offset in range(len(searchable) - pattern_length + 1):
            pattern = searchable[local_offset : local_offset + pattern_length]
            candidate = candidates.setdefault(
                pattern,
                {
                    "pattern": pattern,
                    "alignment_positions": [],
                    "record_ids": set(),
                    "first_record_index": record_index,
                    "first_local_offset": local_offset,
                },
            )
            candidate["alignment_positions"].append(global_start + local_offset)
            candidate["record_ids"].add(int(record["id"]))
        global_start += len(searchable) + 1
    return candidates


def select_natural_pattern(candidates, pattern_length, original_pattern=None):
    """Use the frozen m=1 pattern; otherwise choose support nearest K=16."""
    if pattern_length == 1:
        if original_pattern not in candidates:
            raise ValueError("frozen m=1 pattern is absent from the database")
        return candidates[original_pattern]
    return min(
        candidates.values(),
        key=lambda candidate: (
            abs(len(candidate["record_ids"]) - PUBLIC_K),
            candidate["first_record_index"],
            candidate["first_local_offset"],
            candidate["pattern"],
        ),
    )


def escape_pattern(pattern):
    """Return an ASCII-safe representation of pattern bytes."""
    parts = []
    for value in pattern:
        if value == 0x5C:
            parts.append(r"\\")
        elif 0x20 <= value <= 0x7E:
            parts.append(chr(value))
        else:
            parts.append(f"\\x{value:02x}")
    return "".join(parts)


def build_query_fixtures(source):
    """Reproduce the retained plaintext-only query-selection rule."""
    records = source["workload"]["records"]
    original_pattern = bytes(source["timing_fixture"]["pattern"])
    fixtures = []
    for pattern_length in PATTERN_LENGTHS:
        candidates = enumerate_natural_patterns(records, pattern_length)
        selected = select_natural_pattern(
            candidates,
            pattern_length,
            original_pattern if pattern_length == 1 else None,
        )
        pattern = selected["pattern"]
        fixtures.append(
            {
                "source_query_id": (
                    source["timing_fixture"]["source_query_id"]
                    if pattern_length == 1
                    else f"plaintext-natural-m{pattern_length}"
                ),
                "pattern": list(pattern),
                "pattern_escaped": escape_pattern(pattern),
                "pattern_hex": pattern.hex(),
                "threshold": float(source["timing_fixture"]["threshold"]),
                "expected_literal_alignment_count": len(
                    selected["alignment_positions"]
                ),
                "expected_literal_record_count": len(selected["record_ids"]),
            }
        )
    return fixtures


def build_native_input(source):
    """Create one ring/profile input without mutating the frozen source JSON."""
    return {
        "schema": "cbpir-pattern-length-v1-input",
        "profile": source["profile"],
        "workload": source["workload"],
        "query_fixtures": build_query_fixtures(source),
        "modes": list(MODES),
        "public_k": PUBLIC_K,
        "public_ks": [PUBLIC_K],
        "warmups": WARMUPS,
        "measured_runs": MEASURED_RUNS,
    }


def summarize_durations(rows):
    """Summarize stage and Online definitions without conflating them."""
    if not rows:
        raise ValueError("cannot summarize an empty configuration")
    search = [row["search_query_ns"] + row["search_response_ns"] for row in rows]
    locate = [row["locate_ns"] for row in rows]
    fetch = [
        row["fetch_query_ns"] + row["fetch_response_ns"] + row["recover_ns"]
        for row in rows
    ]
    return {
        "median_search_ns": statistics.median(search),
        "median_locate_ns": statistics.median(locate),
        "median_fetch_ns": statistics.median(fetch),
        "median_per_run_online_sum_ns": statistics.median(
            row["online_sum_ns"] for row in rows
        ),
        "sum_of_stage_medians_ns": (
            statistics.median(search)
            + statistics.median(locate)
            + statistics.median(fetch)
        ),
        "median_online_outer_ns": statistics.median(
            row["online_outer_ns"] for row in rows
        ),
    }


def percentile(values, probability):
    ordered = sorted(values)
    if not ordered:
        raise ValueError("percentile requires at least one value")
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


def workload_path(ring):
    return ROOT / f"data/workloads/formal_{ring}_input.json"


def artifact_paths(ring):
    stem = f"pattern_length_{ring}"
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
        "pattern_length",
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
        raise RuntimeError("release build failed; see evaluation/pattern-length-v1/logs/build.txt")
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


def run_native(ring, payload):
    paths = artifact_paths(ring)
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
            [str(BINARY)],
            cwd=ROOT,
            env=benchmark_env(),
            stdin=stdin,
            stdout=stdout,
            stderr=stderr,
        )
        while process.poll() is None:
            status = process_status(process.pid)
            maximum_rss_kib = max(maximum_rss_kib, status.get("rss_kib", 0))
            maximum_threads = max(maximum_threads, status.get("threads", 0))
            samples += 1
            time.sleep(0.01)
        return_code = process.wait()
    metadata = {
        "ring_dimension": ring,
        "command": str(BINARY),
        "elapsed_seconds": time.monotonic() - started,
        "maximum_rss_kib": maximum_rss_kib,
        "maximum_threads": maximum_threads,
        "observed_process_samples": samples,
        "return_code": return_code,
    }
    write_json(paths["process"], metadata)
    if return_code:
        message = paths["stderr"].read_text(encoding="utf-8", errors="replace")
        raise RuntimeError(f"N={ring} native run failed ({return_code}): {message[-4000:]}")
    return json.loads(paths["output"].read_text(encoding="utf-8"))


def configuration_key(row):
    return (
        int(row["ring_dimension"]),
        row["mode"],
        int(row["pattern_length"]),
        int(row["public_k"]),
    )


def validate_output(output, payload):
    if output["schema"] != "cbpir-pattern-length-v1-output":
        raise ValueError("native output schema drift")
    ring = int(payload["workload"]["ring_dimension"])
    if output["ring_dimension"] != ring or output["openfhe_version"] != "1.5.1":
        raise ValueError("native runtime/profile drift")
    if output["warmups"] != WARMUPS or output["measured_runs"] != MEASURED_RUNS:
        raise ValueError("native repetition contract drift")
    expected_rows = len(PATTERN_LENGTHS) * len(MODES) * MEASURED_RUNS
    if len(output["online_samples"]) != expected_rows:
        raise ValueError("wrong measured-row count")
    if len(output["communication"]) != expected_rows:
        raise ValueError("communication was not serialized for every measured execution")
    measured_correctness = [
        row for row in output["correctness"] if row["row_type"] == "measured"
    ]
    if len(measured_correctness) != expected_rows:
        raise ValueError("wrong correctness-row count")
    if any(not row["success"] for row in output["online_samples"]):
        raise ValueError("one or more online samples failed")
    if any(not row["fetch_exact_reconstruction"] for row in measured_correctness):
        raise ValueError("one or more selected payloads were not reconstructed exactly")
    expected_keys = {
        (ring, mode, pattern_length, PUBLIC_K, run_id)
        for mode in MODES
        for pattern_length in PATTERN_LENGTHS
        for run_id in range(MEASURED_RUNS)
    }
    actual_keys = {
        (*configuration_key(row), int(row["run_id"]))
        for row in output["online_samples"]
    }
    if actual_keys != expected_keys:
        raise ValueError("native configuration/run matrix drift")


def load_outputs():
    outputs = {}
    payloads = {}
    for ring in RINGS:
        paths = artifact_paths(ring)
        payloads[ring] = json.loads(paths["input"].read_text(encoding="utf-8"))
        outputs[ring] = json.loads(paths["output"].read_text(encoding="utf-8"))
        validate_output(outputs[ring], payloads[ring])
    return payloads, outputs


def flatten_rows(outputs):
    rows = []
    for ring, output in outputs.items():
        communication = {
            (*configuration_key(row), int(row["run_id"])): row
            for row in output["communication"]
        }
        correctness = {
            (*configuration_key(row), int(row["run_id"])): row
            for row in output["correctness"]
            if row["row_type"] == "measured"
        }
        for sample in output["online_samples"]:
            key = (*configuration_key(sample), int(sample["run_id"]))
            sizes = communication[key]
            check = correctness[key]
            duration = sample["durations"]
            search_ns = duration["search_query_ns"] + duration["search_response_ns"]
            fetch_ns = (
                duration["fetch_query_ns"]
                + duration["fetch_response_ns"]
                + duration["recover_ns"]
            )
            rows.append(
                {
                    "run_id": sample["run_id"],
                    "ring_dimension": ring,
                    "n": ring // 64,
                    "profile": sample["profile"],
                    "mode": sample["mode"],
                    "query_id": sample["query_id"],
                    "pattern_hex": bytes(sample["pattern"]).hex(),
                    "pattern_length": sample["pattern_length"],
                    "threshold": sample["threshold"],
                    "public_k": sample["public_k"],
                    "candidate_count": sample["candidate_count"],
                    "selected_count": sample["realized_k"],
                    **duration,
                    "search_ns": search_ns,
                    "fetch_ns": fetch_ns,
                    "search_query_bytes": sizes["search_query_bytes"],
                    "search_response_bytes": sizes["search_response_bytes"],
                    "fetch_query_bytes": sizes["fetch_query_bytes"],
                    "fetch_response_bytes": sizes["fetch_response_bytes"],
                    "total_online_bytes": sizes["total_online_bytes"],
                    "fps_oracle_mismatches": check["fps_oracle_mismatches"],
                    "rfps_ckks_threshold_disagreements": check[
                        "rfps_ckks_threshold_disagreements"
                    ],
                    "rfps_cancellation_count": check["rfps_cancellation_count"],
                    "rfps_threshold_only_count": check["rfps_threshold_only_count"],
                    "fetch_exact_reconstruction": check["fetch_exact_reconstruction"],
                    "selected_windows_exact": check["selected_windows_exact"],
                    "unselected_windows_zero": check["unselected_windows_zero"],
                    "unused_tail_zero": check["unused_tail_zero"],
                    "query_ciphertext_count": check["query_ciphertext_count"],
                    "response_ciphertext_count": check["response_ciphertext_count"],
                    "success": sample["success"] and check["success"],
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
        grouped[configuration_key(row)].append(row)
    summaries = []
    byte_fields = (
        "search_query_bytes",
        "search_response_bytes",
        "fetch_query_bytes",
        "fetch_response_bytes",
        "total_online_bytes",
    )
    for key in sorted(grouped):
        group = sorted(grouped[key], key=lambda row: int(row["run_id"]))
        duration_rows = [row for row in group]
        duration_summary = summarize_durations(duration_rows)
        communication_identical = all(
            len({row[field] for row in group}) == 1 for field in byte_fields
        )
        if not communication_identical:
            raise ValueError(f"serialized sizes vary within configuration {key}")
        row = {
            "ring_dimension": key[0],
            "n": key[0] // 64,
            "mode": key[1],
            "pattern_length": key[2],
            "public_k": key[3],
            "runs": len(group),
            "candidate_count_min": min(item["candidate_count"] for item in group),
            "candidate_count_median": statistics.median(
                item["candidate_count"] for item in group
            ),
            "candidate_count_max": max(item["candidate_count"] for item in group),
            "selected_count_min": min(item["selected_count"] for item in group),
            "selected_count_median": statistics.median(
                item["selected_count"] for item in group
            ),
            "selected_count_max": max(item["selected_count"] for item in group),
            "communication_identical": communication_identical,
            "fps_oracle_mismatches": sum(item["fps_oracle_mismatches"] for item in group),
            "rfps_ckks_threshold_disagreements": sum(
                item["rfps_ckks_threshold_disagreements"] for item in group
            ),
            "all_fetch_exact": all(item["fetch_exact_reconstruction"] for item in group),
            "all_success": all(item["success"] for item in group),
            **duration_summary,
        }
        for field in ("search_ns", "locate_ns", "fetch_ns", "online_sum_ns", "online_outer_ns"):
            row.update(metric_summary([item[field] for item in group], field))
        row.update({field: group[0][field] for field in byte_fields})
        summaries.append(row)
    if len(summaries) != 24 or any(row["runs"] != MEASURED_RUNS for row in summaries):
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
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return completed.stdout.strip()


def write_reports(payloads, outputs, build_command):
    rows = flatten_rows(outputs)
    summaries = summarize_rows(rows)
    write_csv(EVIDENCE / "raw.csv", rows)
    write_csv(EVIDENCE / "summary.csv", summaries)
    fixture_document = {
        str(ring): {
            "source_workload": str(workload_path(ring).relative_to(ROOT)),
            "source_sha256": hashlib.sha256(workload_path(ring).read_bytes()).hexdigest(),
            "query_fixtures": payloads[ring]["query_fixtures"],
        }
        for ring in RINGS
    }
    write_json(EVIDENCE / "fixtures.json", fixture_document)
    processes = {
        ring: json.loads(artifact_paths(ring)["process"].read_text(encoding="utf-8"))
        for ring in RINGS
    }
    environment = [
        f"generated_utc={time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}",
        f"platform={platform.platform()}",
        f"python={sys.version.split()[0]}",
        f"cargo={command_text([str(CARGO), '--version'])}",
        f"rustc={command_text([str(RUSTC), '--version'])}",
        "openfhe=1.5.1",
        "omp_num_threads=1",
        "omp_dynamic=FALSE",
        f"build_command={' '.join(map(str, build_command))}",
    ]
    for ring in RINGS:
        environment.extend(
            [
                f"N{ring}_elapsed_seconds={processes[ring]['elapsed_seconds']}",
                f"N{ring}_maximum_rss_kib={processes[ring]['maximum_rss_kib']}",
                f"N{ring}_maximum_threads={processes[ring]['maximum_threads']}",
            ]
        )
    (EVIDENCE / "environment.txt").write_text("\n".join(environment) + "\n", encoding="utf-8")
    verification = [
        "R1.4 pattern-length benchmark verification: PASS",
        f"configurations={len(summaries)}",
        f"measured_executions={len(rows)}",
        "warmups_per_configuration=3",
        "measured_runs_per_configuration=20",
        "communication_serialized_each_execution=true",
        "communication_identical_within_each_configuration=true",
        f"all_fetch_exact={str(all(row['all_fetch_exact'] for row in summaries)).lower()}",
        f"all_configurations_successful={str(all(row['all_success'] for row in summaries)).lower()}",
        "online_statistics=median_per_run_sum,sum_of_stage_medians,median_outer_timer",
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
        for ring in RINGS:
            source = json.loads(workload_path(ring).read_text(encoding="utf-8"))
            payload = build_native_input(source)
            output = run_native(ring, payload)
            validate_output(output, payload)
    payloads, outputs = load_outputs()
    write_reports(payloads, outputs, build_command)
    print(f"validated 24 configurations and {24 * MEASURED_RUNS} measured executions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
