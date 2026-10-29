#!/usr/bin/env python3
"""Run and validate literal record-level Search correctness."""

from __future__ import annotations

import argparse
from collections import Counter
import copy
import csv
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import random
import re
import shutil
import socket
import statistics
import subprocess
import sys
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
if str(ROOT / "scripts/common") not in sys.path:
    sys.path.insert(0, str(ROOT / "scripts/common"))

from workspace_build_lock import shared_workspace_build_lock
FORMAL = ROOT / "evaluation/formal-benchmark"
P0D = ROOT
OPENFHE = ROOT / "openfhe/.openfhe/openfhe-1.5.1/install"
TARGET = ROOT / "target/experiments/search-correctness"
BINARY = TARGET / "release/search_correctness"
EVIDENCE = ROOT / "evaluation/search-correctness"
OUTPUT = ROOT / "target/reproduction/search-correctness"
NATIVE = OUTPUT / "raw/native"
CARGO = Path(shutil.which("cargo") or "cargo")
RUSTC = Path(shutil.which("rustc") or "rustc")
INPUT_SCHEMA = "cbpir-search-correctness-v2-multipattern-input"
OUTPUT_SCHEMA = "cbpir-search-correctness-v2-multipattern-output"
RFPS_SEED = 0xCB_91_27_DF_60_A4_13_5B
RINGS = (8192, 16384, 32768)
PUBLIC_KS = (1, 4, 8, 16)
REPETITIONS = 80
PATTERN_LENGTHS = (1, 4, 8, 16)
BATCH_IDS = (1, 2, 3, 4, 5)
BOOTSTRAP_RESAMPLES = 10_000
BOOTSTRAP_SEED = 0x5E_A2_C0_11


def extract_performance_core_rows(latex: str):
    """Extract only nine computation/communication cells from each Table 2 row."""
    block = latex.split(r"\label{tab:performance}", 1)[1].split(r"\end{tabular}", 1)[0]
    block = "\n".join(line.split("%", 1)[0] for line in block.splitlines())
    rows = []
    for chunk in re.split(r"\\\\", block):
        cells = [re.sub(r"\s+", " ", cell).strip() for cell in chunk.split("&")]
        index = next(
            (i for i, cell in enumerate(cells) if "BF-COIE" in cell or "CBPIR-FPS" in cell or "CBPIR-RFPS" in cell),
            None,
        )
        if index is None or len(cells) < index + 10:
            continue
        protocol = "BF-COIE" if "BF-COIE" in cells[index] else cells[index]
        rows.append({"protocol": protocol, "core_cells": cells[index + 1 : index + 10]})
    return rows


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def searchable_length_range(records):
    lengths = [len(record["searchable"]) for record in records]
    if not lengths:
        raise ValueError("searchable-length range requires records")
    return min(lengths), max(lengths)


def escape_pattern(pattern: bytes):
    """Return an ASCII-safe escaped byte representation."""
    parts = []
    for value in pattern:
        if value == 0x5C:
            parts.append(r"\\")
        elif 0x20 <= value <= 0x7E:
            parts.append(chr(value))
        else:
            parts.append(f"\\x{value:02x}")
    return "".join(parts)


def enumerate_natural_patterns(records, pattern_length):
    """Enumerate natural in-record substrings without Search/RFPS evidence."""
    if pattern_length <= 0:
        raise ValueError("pattern length must be positive")
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
    for candidate in candidates.values():
        candidate["record_ids"] = sorted(candidate["record_ids"])
    return candidates


def select_natural_pattern(candidates, pattern_length, original_pattern=None):
    """Select retained m=1 query or support-nearest natural multi-byte query."""
    if not candidates:
        raise ValueError("no natural pattern candidates")
    if pattern_length == 1:
        if original_pattern not in candidates:
            raise ValueError("original pattern is absent from frozen searchable fields")
        return candidates[original_pattern]
    return min(
        candidates.values(),
        key=lambda candidate: (
            abs(len(candidate["record_ids"]) - 16),
            candidate["first_record_index"],
            candidate["first_local_offset"],
            candidate["pattern"],
        ),
    )


def pattern_support_distribution(candidates):
    return Counter(len(candidate["record_ids"]) for candidate in candidates.values())


def build_query_fixtures(source):
    """Build four plaintext-selected query fixtures and support evidence."""
    records = source["workload"]["records"]
    original_pattern = bytes(source["timing_fixture"]["pattern"])
    fixtures = []
    support_rows = []
    for pattern_length in PATTERN_LENGTHS:
        candidates = enumerate_natural_patterns(records, pattern_length)
        selected = select_natural_pattern(
            candidates,
            pattern_length,
            original_pattern=original_pattern if pattern_length == 1 else None,
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
        distribution = pattern_support_distribution(candidates)
        support_rows.extend(
            {
                "profile": source["profile"],
                "n": source["workload"].get("n", len(records)),
                "pattern_length": pattern_length,
                "distinct_record_support": support,
                "candidate_count": count,
            }
            for support, count in sorted(distribution.items())
        )
    return fixtures, support_rows


def enumerate_literal(records, pattern: bytes):
    """Return literal alignment positions and deduplicated stable record IDs."""
    positions: list[int] = []
    record_ids: list[int] = []
    global_start = 0
    for record in records:
        searchable = bytes(record["searchable"])
        matched = False
        for offset in range(len(searchable) - len(pattern) + 1):
            if searchable[offset : offset + len(pattern)] == pattern:
                positions.append(global_start + offset)
                matched = True
        if matched:
            record_ids.append(int(record["id"]))
        global_start += len(searchable) + 1
    return positions, record_ids


def enumerate_exact_rfps(records, pattern: bytes, randomizer, threshold: float):
    """Return exact RFPS alignments/records plus semantic cause positions."""
    positions: list[int] = []
    record_ids: list[int] = []
    cancellations: list[int] = []
    threshold_only: list[int] = []
    global_start = 0
    for record in records:
        searchable = bytes(record["searchable"])
        accepted_record = False
        for offset in range(len(searchable) - len(pattern) + 1):
            window = searchable[offset : offset + len(pattern)]
            score = sum(
                (left - right) * weight
                for left, right, weight in zip(window, pattern, randomizer)
            )
            accepted = abs(score) <= threshold
            semantic = window == pattern
            position = global_start + offset
            if accepted:
                positions.append(position)
                accepted_record = True
            if not semantic and score == 0:
                cancellations.append(position)
            elif not semantic and score != 0 and accepted:
                threshold_only.append(position)
        if accepted_record:
            record_ids.append(int(record["id"]))
        global_start += len(searchable) + 1
    return positions, record_ids, cancellations, threshold_only


def record_metrics(ground_truth_ids, candidate_ids):
    ground_truth = set(ground_truth_ids)
    candidates = set(candidate_ids)
    true_positive_ids = sorted(ground_truth & candidates)
    false_negative_ids = sorted(ground_truth - candidates)
    false_positive_ids = sorted(candidates - ground_truth)
    tp = len(true_positive_ids)
    fn_count = len(false_negative_ids)
    fp_count = len(false_positive_ids)
    return {
        "true_positive_ids": true_positive_ids,
        "false_negative_ids": false_negative_ids,
        "false_positive_ids": false_positive_ids,
        "tp": tp,
        "fn": fn_count,
        "fp": fp_count,
        "recall": tp / (tp + fn_count) if tp + fn_count else 1.0,
        "precision": tp / (tp + fp_count) if tp + fp_count else None,
    }


def aggregate_counts(rows):
    tp = sum(int(row["tp"]) for row in rows)
    fn_count = sum(int(row["fn_count"]) for row in rows)
    fp_count = sum(int(row["fp_count"]) for row in rows)
    return {
        "tp": tp,
        "gt": tp + fn_count,
        "fn": fn_count,
        "fp": fp_count,
        "recall": tp / (tp + fn_count) if tp + fn_count else 1.0,
        "precision": tp / (tp + fp_count) if tp + fp_count else None,
    }


def exceeds_retained_candidate_range(rows):
    return any(
        row["new_min"] < row["old_min"] or row["new_max"] > row["old_max"]
        for row in rows
    )


def normalize_batch_output(output, batch_id):
    normalized = copy.deepcopy(output)
    normalized["batch_id"] = batch_id
    for row in normalized["runs"]:
        row["batch_id"] = batch_id
    return normalized


def percentile(values, probability):
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * probability
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return ordered[lower] + (ordered[upper] - ordered[lower]) * fraction


def bootstrap_cell(rows, resamples=BOOTSTRAP_RESAMPLES, seed=BOOTSTRAP_SEED):
    """Bootstrap complete Search executions, never individual record IDs."""
    if not rows:
        raise ValueError("bootstrap requires Search executions")
    generator = random.Random(seed)
    recall_samples = []
    precision_samples = []
    for _ in range(resamples):
        sample = [rows[generator.randrange(len(rows))] for _ in range(len(rows))]
        aggregate = aggregate_counts(sample)
        recall_samples.append(aggregate["recall"])
        if aggregate["precision"] is not None:
            precision_samples.append(aggregate["precision"])
    return {
        "resamples": resamples,
        "seed": seed,
        "recall_low": percentile(recall_samples, 0.025),
        "recall_high": percentile(recall_samples, 0.975),
        "precision_low": percentile(precision_samples, 0.025),
        "precision_high": percentile(precision_samples, 0.975),
        "precision_defined_resamples": len(precision_samples),
    }


def summarize_batches(rows):
    summaries = []
    for batch_id in sorted({int(row["batch_id"]) for row in rows}):
        batch_rows = [row for row in rows if int(row["batch_id"]) == batch_id]
        aggregate = aggregate_counts(batch_rows)
        candidate_counts = [len(row["ckks_candidate_record_ids"]) for row in batch_rows]
        summaries.append(
            {
                "batch_id": batch_id,
                "runs": len(batch_rows),
                "TP": aggregate["tp"],
                "GT": aggregate["gt"],
                "FN": aggregate["fn"],
                "FP": aggregate["fp"],
                "recall": aggregate["recall"],
                "precision": aggregate["precision"],
                "candidate_count_min": min(candidate_counts),
                "candidate_count_median": statistics.median(candidate_counts),
                "candidate_count_max": max(candidate_counts),
            }
        )
    return summaries


def splitmix64(state: int):
    state = (state + 0x9E3779B97F4A7C15) & ((1 << 64) - 1)
    value = state
    value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & ((1 << 64) - 1)
    value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & ((1 << 64) - 1)
    return state, value ^ (value >> 31)


def fresh_rfps_weights(seed, length):
    result = []
    rejection_limit = (1 << 64) - 1
    rejection_limit -= rejection_limit % 30
    state = seed
    while len(result) < length:
        state, value = splitmix64(state)
        if value < rejection_limit:
            result.append(value % 30 + 3)
    return result


def formal_seed_schedule(ring_dimension):
    invocation = 0
    result = []
    for public_k in PUBLIC_KS:
        invocation += 3
        for source_run in range(20):
            seed = RFPS_SEED ^ ring_dimension ^ (public_k << 32) ^ invocation
            result.append(
                {
                    "source_public_k": public_k,
                    "source_run": source_run,
                    "invocation": invocation,
                    "seed": seed,
                }
            )
            invocation += 1
    return result


def benchmark_env():
    env = os.environ.copy()
    env.update(
        {
            "OMP_NUM_THREADS": "1",
            "OMP_DYNAMIC": "FALSE",
            "OPENFHE_INSTALL_DIR": str(OPENFHE),
            "LD_LIBRARY_PATH": str(OPENFHE / "lib"),
            "CARGO_TARGET_DIR": str(TARGET),
            "CARGO_BUILD_JOBS": "1",
        }
    )
    return env


def command_output(command):
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return completed.returncode, completed.stdout.strip()


def build_release():
    command = [
        str(CARGO), "build", "--release", "--locked", "--manifest-path",
        str(P0D / "Cargo.toml"), "--features", "native", "--bin",
            "search_correctness",
    ]
    completed = subprocess.run(
        command, cwd=ROOT, env=benchmark_env(), text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    (NATIVE / "build.txt").write_text(completed.stdout, encoding="utf-8")
    if completed.returncode:
        raise RuntimeError("release build failed; see native/build.txt")
    return " ".join(command)


def process_threads(pid):
    try:
        for line in Path(f"/proc/{pid}/status").read_text(encoding="utf-8").splitlines():
            if line.startswith("Threads:"):
                return int(line.split(":", 1)[1])
    except (FileNotFoundError, ProcessLookupError):
        return None
    return None


def write_json(path: Path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def batch_directory(batch_id):
    if batch_id not in BATCH_IDS:
        raise ValueError(f"unsupported batch ID: {batch_id}")
    return NATIVE if batch_id == 1 else NATIVE / f"batch_{batch_id:02d}"


def batch_artifact_paths(batch_id, ring):
    directory = batch_directory(batch_id)
    prefix = f"search_correctness_{ring}"
    return {
        "input": directory / f"{prefix}_input.json",
        "output": directory / f"{prefix}_output.json",
        "stderr": directory / f"{prefix}_stderr.txt",
        "process": directory / f"{prefix}_process.json",
    }


def run_native(batch_id, ring, payload):
    paths = batch_artifact_paths(batch_id, ring)
    paths["input"].parent.mkdir(parents=True, exist_ok=True)
    existing = [path for path in paths.values() if path.exists()]
    if existing:
        names = ", ".join(str(path.relative_to(ROOT)) for path in existing)
        raise FileExistsError(f"refusing to overwrite batch artifacts: {names}")
    input_path = paths["input"]
    output_path = paths["output"]
    stderr_path = paths["stderr"]
    process_path = paths["process"]
    write_json(input_path, payload)
    started = time.monotonic()
    with input_path.open("rb") as stdin, output_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        process = subprocess.Popen(
            [str(BINARY)], cwd=ROOT, env=benchmark_env(), stdin=stdin, stdout=stdout, stderr=stderr
        )
        thread_samples = []
        while process.poll() is None:
            count = process_threads(process.pid)
            if count is not None:
                thread_samples.append(count)
            time.sleep(0.005)
        return_code = process.wait()
    elapsed = time.monotonic() - started
    metadata = {
        "batch_id": batch_id,
        "ring_dimension": ring,
        "command": str(BINARY),
        "elapsed_seconds": elapsed,
        "input": str(input_path.relative_to(ROOT)),
        "maximum_threads": max(thread_samples) if thread_samples else None,
        "minimum_threads": min(thread_samples) if thread_samples else None,
        "observed_thread_samples": len(thread_samples),
        "output": str(output_path.relative_to(ROOT)),
        "return_code": return_code,
        "stderr": str(stderr_path.relative_to(ROOT)),
    }
    write_json(process_path, metadata)
    if return_code:
        message = stderr_path.read_text(encoding="utf-8", errors="replace")
        raise RuntimeError(f"N={ring} native run failed ({return_code}): {message[-4000:]}")
    return normalize_batch_output(
        json.loads(output_path.read_text(encoding="utf-8")), batch_id
    ), metadata


def load_batch_artifacts(batch_id, ring):
    paths = batch_artifact_paths(batch_id, ring)
    missing = [path for path in paths.values() if not path.exists()]
    if missing:
        names = ", ".join(str(path.relative_to(ROOT)) for path in missing)
        raise FileNotFoundError(f"missing batch artifacts: {names}")
    output = normalize_batch_output(
        json.loads(paths["output"].read_text(encoding="utf-8")), batch_id
    )
    process = json.loads(paths["process"].read_text(encoding="utf-8"))
    process["batch_id"] = batch_id
    process["ring_dimension"] = ring
    return output, process


def validate_batch_inputs(inputs):
    """Require every retained batch to use the exact generated workload JSON."""
    rows = []
    for ring in RINGS:
        expected = json.dumps(inputs[ring], indent=2, sort_keys=True) + "\n"
        expected_hash = hashlib.sha256(expected.encode("utf-8")).hexdigest()
        for batch_id in BATCH_IDS:
            path = batch_artifact_paths(batch_id, ring)["input"]
            actual = path.read_text(encoding="utf-8")
            actual_hash = hashlib.sha256(actual.encode("utf-8")).hexdigest()
            if actual != expected:
                raise ValueError(
                    f"batch {batch_id} N={ring} input differs from generated workload"
                )
            rows.append(
                {
                    "batch_id": batch_id,
                    "N": ring,
                    "sha256": actual_hash,
                    "matches_generated": actual_hash == expected_hash,
                }
            )
    return rows


def frozen_randomizers(ring):
    output = json.loads(
        (FORMAL / "native" / f"formal_{ring}_output.json").read_text(encoding="utf-8")
    )
    return {
        (int(row["public_k"]), int(row["run_id"])): row["randomizer"]
        for row in output["online_samples"] if row["mode"] == "rfps"
    }


def validate_profile(output, payload):
    if output["schema"] != OUTPUT_SCHEMA or output["ring_dimension"] != payload["workload"]["ring_dimension"]:
        raise ValueError("native output schema/profile drift")
    if (
        output["repetitions_per_mode"] != REPETITIONS
        or output["public_ks_checked"] != list(PUBLIC_KS)
        or output["pattern_lengths"] != list(PATTERN_LENGTHS)
    ):
        raise ValueError("native repetition/K contract drift")
    if output["openfhe_version"] != "1.5.1":
        raise ValueError("OpenFHE version drift")
    rows = output["runs"]
    if len(rows) != 2 * REPETITIONS * len(PATTERN_LENGTHS):
        raise ValueError("wrong row count for native profile")
    records = payload["workload"]["records"]
    fixtures = {
        len(fixture["pattern"]): fixture for fixture in payload["query_fixtures"]
    }
    old_randomizers = frozen_randomizers(output["ring_dimension"])
    for row in rows:
        fixture = fixtures[row["pattern_length"]]
        pattern = bytes(fixture["pattern"])
        literal_positions, literal_ids = enumerate_literal(records, pattern)
        if (
            row["pattern"] != list(pattern)
            or row["pattern_hex"] != pattern.hex()
            or row["pattern_escaped"] != escape_pattern(pattern)
        ):
            raise ValueError("native pattern provenance drift")
        if row["literal_alignment_positions"] != literal_positions or row["literal_record_ids"] != literal_ids:
            raise ValueError("Rust literal oracle differs from independent enumeration")
        if not row["k_invariant"]:
            raise ValueError("candidate generation changed with K")
        metrics = record_metrics(literal_ids, row["ckks_candidate_record_ids"])
        expected = (
            metrics["true_positive_ids"], metrics["false_negative_ids"],
            metrics["false_positive_ids"], metrics["tp"], metrics["fn"], metrics["fp"],
        )
        actual = (
            row["true_positive_record_ids"], row["false_negative_record_ids"],
            row["false_positive_record_ids"], row["tp"], row["fn_count"], row["fp_count"],
        )
        if actual != expected:
            raise ValueError("native confusion matrix does not follow from ID sets")
        if len(row["ckks_candidate_record_ids"]) != len(set(row["ckks_candidate_record_ids"])):
            raise ValueError("candidate record IDs were not deduplicated")
        if row["mode"] == "fps":
            if row["seed"] is not None or row["randomizer"] or row["exact_rfps_record_ids"] is not None:
                raise ValueError("FPS row contains RFPS state")
        else:
            key = (row["seed_source_public_k"], row["seed_source_run"])
            expected_randomizer = fresh_rfps_weights(row["seed"], len(pattern))
            if expected_randomizer != row["randomizer"]:
                raise ValueError("RFPS randomizer differs from recorded seed")
            if old_randomizers[key] != expected_randomizer[:1]:
                raise ValueError("RFPS seed prefix differs from frozen formal matrix")
            exact_positions, exact_ids, cancellations, threshold_only = enumerate_exact_rfps(
                records, pattern, row["randomizer"], row["threshold"]
            )
            if row["exact_rfps_alignment_positions"] != exact_positions:
                raise ValueError("exact RFPS alignment set mismatch")
            if row["exact_rfps_record_ids"] != exact_ids:
                raise ValueError("exact RFPS record set mismatch")
            if row["cancellation_alignment_positions"] != cancellations:
                raise ValueError("RFPS cancellation attribution mismatch")
            if row["threshold_only_alignment_positions"] != threshold_only:
                raise ValueError("RFPS threshold-only attribution mismatch")
    return literal_positions, literal_ids


def json_field(value):
    return json.dumps(value, separators=(",", ":"))


def write_csv(path: Path, rows, fieldnames):
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def verify_frozen_manifest():
    manifest = EVIDENCE / "validation/frozen_hashes_before.sha256"
    expected_paths = set()
    for line in manifest.read_text(encoding="utf-8").splitlines():
        expected_hash, relative = line.split("  ", 1)
        path = ROOT / relative
        expected_paths.add(relative)
        if not path.is_file() or sha256_file(path) != expected_hash:
            raise ValueError(f"frozen evidence changed: {relative}")
    frozen_roots = (
        FORMAL / "frozen-inputs",
        FORMAL / "raw",
        FORMAL / "results",
    )
    current_paths = {
        path.relative_to(ROOT).as_posix()
        for directory in frozen_roots
        for path in directory.rglob("*")
        if path.is_file()
        and "__pycache__" not in path.parts
        and path.suffix not in {".pyc", ".pyo"}
    }
    if current_paths != expected_paths:
        added = sorted(current_paths - expected_paths)
        removed = sorted(expected_paths - current_paths)
        raise ValueError(f"frozen evidence file-set changed: added={added}, removed={removed}")
    return len(expected_paths)


def verify_table2_snapshot():
    before_path = EVIDENCE / "validation/09_table2_core_before.json"
    after_path = EVIDENCE / "validation/10_table2_core_after.json"
    before = json.loads(before_path.read_text(encoding="utf-8"))
    after = json.loads(after_path.read_text(encoding="utf-8"))
    if after != before:
        raise ValueError("Table 2 computation/communication core changed")
    return len(after)


def format_percent(value):
    return "--" if value is None else f"{100.0 * float(value):.3f}%"


def flatten_row(row):
    result = dict(row)
    for key, value in result.items():
        if isinstance(value, list):
            result[key] = json_field(value)
    return result


def capture_environment(processes):
    commands = {
        "cargo": [str(CARGO), "--version"],
        "rustc": [str(RUSTC), "--version", "--verbose"],
        "cmake": ["cmake", "--version"],
        "compiler": ["c++", "--version"],
    }
    details = []
    for label, command in commands.items():
        code, output = command_output(command)
        details.extend([f"[{label}]", f"exit={code}", output, ""])
    cpu_model = "unknown"
    for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
        if line.startswith("model name"):
            cpu_model = line.split(":", 1)[1].strip()
            break
    memory = next(
        line for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines()
        if line.startswith("MemTotal:")
    )
    os_release = Path("/etc/os-release").read_text(encoding="utf-8").strip()
    text = [
        f"timestamp_utc={dt.datetime.now(dt.timezone.utc).isoformat()}",
        f"hostname={socket.gethostname()}", f"platform={platform.platform()}",
        f"kernel={platform.release()}", f"cpu_model={cpu_model}",
        f"logical_cpu_count={os.cpu_count()}", f"memory={memory}",
        "OMP_NUM_THREADS=1", "OMP_DYNAMIC=FALSE", "OpenFHE=1.5.1",
        "execution=serial native processes", "timing_use=none; correctness-only rerun",
        "", os_release, "", *details, "[process_thread_observations]",
        json.dumps(processes, indent=2, sort_keys=True),
    ]
    (OUTPUT / "00_environment.txt").write_text("\n".join(text) + "\n", encoding="utf-8")


def write_reports(inputs, outputs, processes, build_command, support_rows):
    all_rows = [
        row
        for batch_id in BATCH_IDS
        for ring in RINGS
        for row in outputs[batch_id][ring]["runs"]
    ]
    expected_rows = len(BATCH_IDS) * len(RINGS) * len(PATTERN_LENGTHS) * 2 * REPETITIONS
    if len(all_rows) != expected_rows:
        raise ValueError(f"expected {expected_rows} raw rows, found {len(all_rows)}")
    ground_truth_rows = []
    ground_truth_markdown = [
        "# Literal Record-Level Ground Truth", "",
        "Enumeration scans only windows wholly inside each frozen record. Multiple matches in one record map to one stable record ID.",
        "", "| Profile | N | n | Pattern | Alignments | Record IDs | Records |",
        "|---|---:|---:|---|---:|---|---:|",
    ]
    manifest_rows = []
    for ring in RINGS:
        payload = inputs[ring]
        minimum, maximum = searchable_length_range(payload["workload"]["records"])
        for fixture in payload["query_fixtures"]:
            pattern = bytes(fixture["pattern"])
            positions, ids = enumerate_literal(payload["workload"]["records"], pattern)
            row = {
                "profile": payload["profile"], "N": ring,
                "n": payload["workload"]["n"],
                "searchable_length_min": minimum,
                "searchable_length_max": maximum,
                "pattern_hex": pattern.hex(),
                "pattern_escaped": escape_pattern(pattern),
                "pattern_length": len(pattern),
                "literal_alignment_positions": json_field(positions),
                "literal_alignment_count": len(positions),
                "literal_record_ids": json_field(ids),
                "literal_record_count": len(ids),
            }
            ground_truth_rows.append(row)
            manifest_rows.append(
                {
                    **row,
                    "selection_rule": (
                        "retained frozen m=1 query"
                        if len(pattern) == 1
                        else "closest record support to 16; earliest occurrence; byte order"
                    ),
                    "selection_evidence": "plaintext frozen searchable fields only",
                }
            )
            ground_truth_markdown.append(
                f"| {payload['profile']} | {ring} | {payload['workload']['n']} | "
                f"`0x{pattern.hex()}` (`{escape_pattern(pattern)}`) | "
                f"{len(positions)} | `{ids}` | {len(ids)} |"
            )
    write_csv(OUTPUT / "01_literal_ground_truth.csv", ground_truth_rows, list(ground_truth_rows[0]))
    (OUTPUT / "02_literal_ground_truth.md").write_text("\n".join(ground_truth_markdown) + "\n", encoding="utf-8")
    write_csv(OUTPUT / "12_search_workload_manifest.csv", manifest_rows, list(manifest_rows[0]))
    write_csv(
        OUTPUT / "13_pattern_support_distribution.csv",
        support_rows,
        list(support_rows[0]),
    )
    with (OUTPUT / "03_raw_search_correctness.jsonl").open("w", encoding="utf-8") as handle:
        for row in all_rows:
            handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
    flat_rows = [flatten_row(row) for row in all_rows]
    write_csv(OUTPUT / "04_raw_search_correctness.csv", flat_rows, list(flat_rows[0]))

    summary_rows = []
    batch_summary_rows = []
    bootstrap_rows = []
    for ring in RINGS:
        for pattern_length in PATTERN_LENGTHS:
            for mode in ("fps", "rfps"):
                rows = [
                    row for row in all_rows
                    if row["ring_dimension"] == ring
                    if row["mode"] == mode and row["pattern_length"] == pattern_length
                ]
                if len(rows) != len(BATCH_IDS) * REPETITIONS:
                    raise ValueError("wrong per-cell repetition count")
                aggregate = aggregate_counts(rows)
                bootstrap_seed = (
                    BOOTSTRAP_SEED
                    ^ (ring << 16)
                    ^ (pattern_length << 8)
                    ^ (0 if mode == "fps" else 1)
                )
                bootstrap = bootstrap_cell(rows, seed=bootstrap_seed)
                per_batch = summarize_batches(rows)
                if [row["batch_id"] for row in per_batch] != list(BATCH_IDS):
                    raise ValueError("cell does not contain all five batches")
                if any(row["runs"] != REPETITIONS for row in per_batch):
                    raise ValueError("wrong per-batch repetition count")
                profile = outputs[1][ring]["profile"]
                n = outputs[1][ring]["n"]
                common = {
                    "profile": profile,
                    "N": ring,
                    "n": n,
                    "pattern_length": pattern_length,
                    "pattern_hex": rows[0]["pattern_hex"],
                    "pattern_escaped": rows[0]["pattern_escaped"],
                    "mode": mode.upper(),
                }
                summary_rows.append(
                    {
                        **common,
                        "searchable_length_min": min(
                            len(record["searchable"])
                            for record in inputs[ring]["workload"]["records"]
                        ),
                        "searchable_length_max": max(
                            len(record["searchable"])
                            for record in inputs[ring]["workload"]["records"]
                        ),
                        "runs": len(rows),
                        "TP": aggregate["tp"], "GT": aggregate["gt"],
                        "FN": aggregate["fn"], "FP": aggregate["fp"],
                        "recall": f"{aggregate['recall']:.12f}",
                        "precision": "" if aggregate["precision"] is None else f"{aggregate['precision']:.12f}",
                        "recall_ci95_low": f"{bootstrap['recall_low']:.12f}",
                        "recall_ci95_high": f"{bootstrap['recall_high']:.12f}",
                        "precision_ci95_low": "" if bootstrap["precision_low"] is None else f"{bootstrap['precision_low']:.12f}",
                        "precision_ci95_high": "" if bootstrap["precision_high"] is None else f"{bootstrap['precision_high']:.12f}",
                        "all_k_invariant": all(row["k_invariant"] for row in rows),
                        "metric_boundary": "pre-First_K",
                        "ground_truth": "literal record IDs",
                    }
                )
                bootstrap_rows.append(
                    {
                        **common,
                        "runs": len(rows),
                        "bootstrap_resamples": bootstrap["resamples"],
                        "bootstrap_seed": bootstrap["seed"],
                        "recall": f"{aggregate['recall']:.12f}",
                        "recall_ci95_low": f"{bootstrap['recall_low']:.12f}",
                        "recall_ci95_high": f"{bootstrap['recall_high']:.12f}",
                        "precision": "" if aggregate["precision"] is None else f"{aggregate['precision']:.12f}",
                        "precision_ci95_low": "" if bootstrap["precision_low"] is None else f"{bootstrap['precision_low']:.12f}",
                        "precision_ci95_high": "" if bootstrap["precision_high"] is None else f"{bootstrap['precision_high']:.12f}",
                        "precision_defined_resamples": bootstrap["precision_defined_resamples"],
                        "statistical_unit": "complete Search execution",
                    }
                )
                for batch in per_batch:
                    batch_summary_rows.append(
                        {
                            **common,
                            **batch,
                            "recall": f"{batch['recall']:.12f}",
                            "precision": "" if batch["precision"] is None else f"{batch['precision']:.12f}",
                        }
                    )
    write_csv(OUTPUT / "05_search_correctness_summary.csv", summary_rows, list(summary_rows[0]))
    write_csv(OUTPUT / "14_batch_summary.csv", batch_summary_rows, list(batch_summary_rows[0]))
    write_csv(OUTPUT / "15_bootstrap_summary.csv", bootstrap_rows, list(bootstrap_rows[0]))

    attribution_rows = []
    for ring in RINGS:
        for pattern_length in PATTERN_LENGTHS:
            rows = [
                row for row in all_rows
                if row["ring_dimension"] == ring
                if row["mode"] == "rfps" and row["pattern_length"] == pattern_length
            ]
            attribution_rows.append(
                {
                "profile": outputs[1][ring]["profile"], "N": ring,
                "n": outputs[1][ring]["n"], "pattern_length": pattern_length,
                "pattern_hex": rows[0]["pattern_hex"],
                "pattern_escaped": rows[0]["pattern_escaped"], "runs": len(rows),
                "exact_semantic_fp_record_instances": sum(len(row["rfps_exact_semantic_fp_records"]) for row in rows),
                "ckks_fp_record_instances_vs_exact_rfps": sum(len(row["rfps_ckks_fp_records"]) for row in rows),
                "ckks_fn_record_instances_vs_exact_rfps": sum(len(row["rfps_ckks_fn_records"]) for row in rows),
                "final_fp_record_instances_vs_literal": sum(row["fp_count"] for row in rows),
                "final_fn_record_instances_vs_literal": sum(row["fn_count"] for row in rows),
                "cancellation_alignment_instances": sum(len(row["cancellation_alignment_positions"]) for row in rows),
                "threshold_only_alignment_instances": sum(len(row["threshold_only_alignment_positions"]) for row in rows),
                "ckks_fp_alignment_instances_vs_exact_rfps": sum(len(row["ckks_false_positive_alignment_positions"]) for row in rows),
                "ckks_fn_alignment_instances_vs_exact_rfps": sum(len(row["ckks_false_negative_alignment_positions"]) for row in rows),
                }
            )
    write_csv(OUTPUT / "06_rfps_error_attribution.csv", attribution_rows, list(attribution_rows[0]))

    frozen_variation = []
    for ring in RINGS:
        old = json.loads((FORMAL / "native" / f"formal_{ring}_output.json").read_text(encoding="utf-8"))
        old_counts = [row["candidate_count"] for row in old["online_samples"] if row["mode"] == "rfps"]
        new_counts = [
            len(row["ckks_candidate_record_ids"])
            for row in all_rows
            if row["ring_dimension"] == ring
            and row["mode"] == "rfps"
            and row["pattern_length"] == 1
        ]
        frozen_variation.append(
            {"N": ring, "old_min": min(old_counts), "old_max": max(old_counts),
             "new_min": min(new_counts), "new_max": max(new_counts)}
        )
    report = [
        "# Fixed Replication Study", "",
        "This report pools five independent fresh-encryption batches of 80 complete Search executions per `(n,m,mode)` cell. The statistical unit is one Search execution. Confidence intervals use 10,000 nonparametric bootstrap resamples of complete executions.",
        "", "## Final 24 cells", "",
        "| n | m | Mode | TP/GT | FN | FP | Recall [95% CI] | Precision [95% CI] | Five batch recalls | Five batch precisions |",
        "|---:|---:|---|---:|---:|---:|---|---|---|---|",
    ]
    for row in summary_rows:
        batches = [
            batch for batch in batch_summary_rows
            if batch["N"] == row["N"]
            and batch["pattern_length"] == row["pattern_length"]
            and batch["mode"] == row["mode"]
        ]
        recalls = ", ".join(format_percent(batch["recall"]) for batch in batches)
        precisions = ", ".join(format_percent(batch["precision"]) for batch in batches)
        report.append(
            f"| {row['n']} | {row['pattern_length']} | {row['mode']} | "
            f"{row['TP']}/{row['GT']} | {row['FN']} | {row['FP']} | "
            f"{format_percent(row['recall'])} "
            f"[{format_percent(row['recall_ci95_low'])}, {format_percent(row['recall_ci95_high'])}] | "
            f"{format_percent(row['precision'])} "
            f"[{format_percent(row['precision_ci95_low'])}, {format_percent(row['precision_ci95_high'])}] | "
            f"{recalls} | {precisions} |"
        )
    report.extend(
        [
            "", "## Fixed workloads", "",
            "Patterns were selected from plaintext frozen searchable fields before any RFPS or CKKS result existed. For `m>1`, selection minimized distance between distinct-record support and 16, then used earliest occurrence and byte order as deterministic tie breaks. The original frozen query was retained for `m=1`.",
            "", "| n | m | Escaped pattern | Hex | Literal alignments | Literal records |",
            "|---:|---:|---|---|---:|---:|",
        ]
    )
    for row in manifest_rows:
        report.append(
            f"| {row['n']} | {row['pattern_length']} | `{row['pattern_escaped']}` | "
            f"`{row['pattern_hex']}` | {row['literal_alignment_count']} | {row['literal_record_count']} |"
        )
    report.extend(
        [
            "", "## RFPS attribution", "",
            "Exact RFPS uses each run's same randomizer and threshold. It is an attribution layer, not semantic ground truth. Alignment counts and record-instance counts remain separate.",
            "", "| n | m | Cancellation align. | Threshold-only align. | Exact-semantic FP records | CKKS FP align. | CKKS FN align. | CKKS FP records | CKKS FN records | Final literal FP | Final literal FN |",
            "|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for row in attribution_rows:
        report.append(
            f"| {row['n']} | {row['pattern_length']} | "
            f"{row['cancellation_alignment_instances']} | {row['threshold_only_alignment_instances']} | "
            f"{row['exact_semantic_fp_record_instances']} | "
            f"{row['ckks_fp_alignment_instances_vs_exact_rfps']} | "
            f"{row['ckks_fn_alignment_instances_vs_exact_rfps']} | "
            f"{row['ckks_fp_record_instances_vs_exact_rfps']} | "
            f"{row['ckks_fn_record_instances_vs_exact_rfps']} | "
            f"{row['final_fp_record_instances_vs_literal']} | "
            f"{row['final_fn_record_instances_vs_literal']} |"
        )
    pilot_m1 = {
        8192: {"GT": 1280, "FN": 0},
        16384: {"GT": 1280, "FN": 2},
        32768: {"GT": 1360, "FN": 217},
    }
    report.extend(
        [
            "", "## `m=1` replication stability", "",
            "The earlier six-cell pilot is external replication evidence only and is not pooled. Its reviewed RFPS FN/GT values are compared with each new batch below. More runs characterize, but do not remove, fresh-encryption variability.",
            "", "| n | Earlier pilot FN/GT | Five batch FN/GT | Five batch candidate ranges | Pooled recall [95% CI] |",
            "|---:|---:|---|---|---|",
        ]
    )
    for ring in RINGS:
        row = next(
            item for item in summary_rows
            if item["N"] == ring and item["pattern_length"] == 1 and item["mode"] == "RFPS"
        )
        batches = [
            item for item in batch_summary_rows
            if item["N"] == ring and item["pattern_length"] == 1 and item["mode"] == "RFPS"
        ]
        batch_counts = ", ".join(f"{item['FN']}/{item['GT']}" for item in batches)
        candidate_ranges = ", ".join(
            f"{item['candidate_count_min']}--{item['candidate_count_max']}" for item in batches
        )
        pilot = pilot_m1[ring]
        report.append(
            f"| {row['n']} | {pilot['FN']}/{pilot['GT']} | {batch_counts} | "
            f"{candidate_ranges} | {format_percent(row['recall'])} "
            f"[{format_percent(row['recall_ci95_low'])}, {format_percent(row['recall_ci95_high'])}] |"
        )
    report.extend(
        [
            "", "## Interpretation", "",
            "FPS and RFPS results above use literal record matching before `First_K`. Repeated matches in one record count once. All candidate sets were invariant when passed to `K in {1,4,8,16}`; only later truncation changed. Exact-RFPS cancellation explains intrinsic false positives where present. Threshold-only acceptance remains zero under the retained `tau=0.5` because exact RFPS scores are integral. CKKS effects remain separately reported. Manuscript integration is intentionally deferred for review.",
        ]
    )
    (OUTPUT / "16_replication_report.md").write_text("\n".join(report) + "\n", encoding="utf-8")

    frozen_file_count = verify_frozen_manifest()
    table2_rows = verify_table2_snapshot()
    all_processes_single_thread = all(
        process.get("minimum_threads") == 1 and process.get("maximum_threads") == 1
        for process in processes
    )
    if not all_processes_single_thread:
        raise ValueError("external thread observation was not exactly one")
    validation = [
        "# Final Replication Validation", "", "## Gate result", "",
        "PASS FOR STATISTICAL REVIEW. Manuscript remains unchanged pending approval.",
        "", "## Checks", "",
        f"- Exactly {len(all_rows):,} Search-only executions: 3 profiles x 4 pattern lengths x 2 modes x 5 batches x 80 runs.",
        "- Exactly 400 final runs exist per `(n,m,mode)` cell and 80 per batch/cell.",
        "- Every run uses its plaintext-selected frozen pattern; no RFPS/CKKS result affected selection.",
        "- Literal IDs independently enumerated from frozen bytes matched every native row.",
        "- Every TP/FN/FP value was recomputed from serialized record-ID sets.",
        "- Exact RFPS reused the same recorded randomizer and `tau` as CKKS RFPS.",
        "- Metrics use deduplicated candidate records before `First_K`.",
        "- Candidate generation was invariant for K=1,4,8,16 in every run.",
        "- Batch 1 and batches 2--5 used byte-identical generated input JSON for each profile.",
        "- Every batch/profile ran as a separate native process using the same release-binary path; each run constructed a fresh encrypted query inside that process.",
        "- `batch_id` was added only while normalizing completed native output; it was not part of any cryptographic input or computation.",
        "- No result was discarded as anomalous.",
        "- Per-run, per-batch, and pooled summaries reproduce from raw JSONL/CSV.",
        f"- All {len(processes)} native processes were externally observed at exactly one thread.",
        f"- All {frozen_file_count} frozen-evidence files and their file set match the pre-run manifest.",
        f"- All {table2_rows} Table 2 protocol rows retain byte-identical computation/communication core cells.",
        "- No Fetch procedure, performance aggregation, manuscript edit, or PDF build occurred.",
        "", "## Retained versus five-batch `m=1` candidate ranges", "",
        "| N | Retained formal range | Five-batch range |", "|---:|---:|---:|",
    ]
    validation.extend(
        f"| {row['N']} | {row['old_min']}--{row['old_max']} | {row['new_min']}--{row['new_max']} |"
        for row in frozen_variation
    )
    validation.extend(
        [
            "", "Range differences are expected evidence of fresh-encryption threshold variability, not a reason to discard runs. Final inference uses all 400 predefined runs per cell and run-level bootstrap uncertainty.",
        ]
    )
    (OUTPUT / "17_final_validation.md").write_text("\n".join(validation) + "\n", encoding="utf-8")
    commands = [
        "# From repository root:",
        "PYTHONDONTWRITEBYTECODE=1 python3 scripts/search-correctness/test_contract.py",
        "OMP_NUM_THREADS=1 OMP_DYNAMIC=FALSE PYTHONDONTWRITEBYTECODE=1 python3 scripts/search-correctness/run.py --run-additional-batches",
        "OMP_NUM_THREADS=1 OMP_DYNAMIC=FALSE PYTHONDONTWRITEBYTECODE=1 python3 scripts/search-correctness/run.py --validate-only",
        "", f"# Native build used by orchestrator:\n{build_command}",
    ]
    (OUTPUT / "08_reproduction_commands.txt").write_text("\n".join(commands) + "\n", encoding="utf-8")
    capture_environment(processes)
    return summary_rows, attribution_rows, batch_summary_rows, bootstrap_rows


def sha256_file(path: Path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_result_checksums():
    excluded = {"results_checksums.sha256", "frozen_hashes_after.sha256"}
    paths = [
        path for path in sorted(OUTPUT.rglob("*"))
        if path.is_file() and path.name not in excluded and ".pyc" not in path.name
    ]
    lines = [f"{sha256_file(path)}  {path.relative_to(OUTPUT).as_posix()}" for path in paths]
    (OUTPUT / "results_checksums.sha256").write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_source_provenance():
    source_files = (
        Path("Cargo.toml"),
        Path("Cargo.lock"),
        Path("src/lib.rs"),
        Path("src/search_correctness.rs"),
        Path("src/bin/search_correctness.rs"),
        Path("scripts/benchmark/formal_workload.py"),
        Path("scripts/benchmark/final_workload.py"),
    )
    head_code, head = command_output(["git", "rev-parse", "HEAD"])
    branch_code, branch = command_output(["git", "branch", "--show-current"])
    lines = [
        f"repository_head={head if head_code == 0 else 'unavailable'}",
        f"repository_branch={branch if branch_code == 0 else 'unavailable'}",
        "OpenFHE=1.5.1",
        "OpenFHE-rs=3e6a46b1377e89e09414cbca7c0c0140c5338397",
        "OpenFHE-core=1306d14f8c26bb6150d3e6ad54f28dfe1007689e",
        "",
        "sha256  source",
    ]
    lines.extend(f"{sha256_file(ROOT / path)}  {path.as_posix()}" for path in source_files)
    (OUTPUT / "11_source_provenance.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def build_inputs():
    formal_workload = _load_module(
        "search_correctness_formal_workload",
        ROOT / "scripts/benchmark/formal_workload.py",
    )
    formal_inputs = formal_workload.build_inputs(
        warmups=0, measured_runs=REPETITIONS, modes=["fps", "rfps"], public_ks=list(PUBLIC_KS)
    )
    result = {}
    support_rows = []
    for ring, source in formal_inputs.items():
        query_fixtures, profile_support = build_query_fixtures(source)
        support_rows.extend(profile_support)
        result[ring] = {
            "schema": INPUT_SCHEMA, "profile": source["profile"],
            "workload": source["workload"], "query_fixtures": query_fixtures,
            "modes": ["fps", "rfps"], "public_ks": list(PUBLIC_KS),
            "repetitions": REPETITIONS,
        }
    return result, support_rows


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--run-additional-batches", action="store_true")
    mode.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    if args.validate_only:
        subprocess.run([sys.executable, str(HERE / "test_contract.py")], check=True)
        frozen_files = verify_frozen_manifest()
        table_rows = verify_table2_snapshot()
        print(
            json.dumps(
                {
                    "status": "retained evidence validated",
                    "frozen_files": frozen_files,
                    "table2_rows": table_rows,
                },
                indent=2,
            )
        )
        return
    NATIVE.mkdir(parents=True, exist_ok=True)
    inputs, support_rows = build_inputs()
    build_command = (
        f"{CARGO} build --release --locked --manifest-path {P0D / 'Cargo.toml'} "
        "--features native --bin search_correctness"
    )
    for ring in RINGS:
        batch_one_input = json.loads(
            batch_artifact_paths(1, ring)["input"].read_text(encoding="utf-8")
        )
        if batch_one_input != inputs[ring]:
            raise ValueError(f"batch 1 N={ring} input differs from generated workload")
    if args.run_additional_batches:
        with shared_workspace_build_lock():
            build_command = build_release()
            for batch_id in BATCH_IDS[1:]:
                for ring in RINGS:
                    run_native(batch_id, ring, inputs[ring])

    outputs = {batch_id: {} for batch_id in BATCH_IDS}
    processes = []
    for batch_id in BATCH_IDS:
        for ring in RINGS:
            output, process = load_batch_artifacts(batch_id, ring)
            outputs[batch_id][ring] = output
            processes.append(process)
            validate_profile(output, inputs[ring])
    input_hash_rows = validate_batch_inputs(inputs)
    write_csv(
        OUTPUT / "18_batch_input_hashes.csv",
        input_hash_rows,
        list(input_hash_rows[0]),
    )
    summary, attribution, batch_summary, bootstrap = write_reports(
        inputs, outputs, processes, build_command, support_rows
    )
    write_source_provenance()
    write_result_checksums()
    print(
        json.dumps(
            {
                "raw_executions": sum(row["runs"] for row in summary),
                "final_cells": len(summary),
                "batch_rows": len(batch_summary),
                "bootstrap_rows": len(bootstrap),
                "rfps_attribution_rows": len(attribution),
                "report": str((OUTPUT / "16_replication_report.md").relative_to(ROOT)),
                "validation": str((OUTPUT / "17_final_validation.md").relative_to(ROOT)),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
