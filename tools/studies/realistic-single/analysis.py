"""Validate native BGL K=1 output against an independent byte oracle."""
from collections import defaultdict
import csv
import json
import math
from pathlib import Path
import statistics
import sys

from generate import SCHEMA, evaluate


RESULT_SCHEMA = "p0d-bgl-k1-native-result-v1"
METADATA_SCHEMA = "p0d-bgl-k1-native-metadata-v1"


def _score(window, pattern, mode, weights):
    if mode == "fps":
        return sum((left - right) ** 2 for left, right in zip(window, pattern))
    return sum(
        (left - right) * weight
        for left, right, weight in zip(window, pattern, weights)
    )


def independent_windows(records, pattern, mode, weights, threshold):
    """Label every framed offset without using P0-D Catalog or Locate."""
    pattern = bytes(pattern)
    framed = bytearray()
    spans = []
    for index, record in enumerate(records):
        start = len(framed)
        framed.extend(record["searchable"])
        spans.append((start, len(framed), record["id"], index))
        if index + 1 < len(records):
            framed.append(0)
    rows = []
    for position in range(max(0, len(framed) - len(pattern) + 1)):
        end = position + len(pattern)
        containing = next((span for span in spans if span[0] <= position and end <= span[1]), None)
        window = bytes(framed[position:end])
        score = _score(window, pattern, mode, weights)
        rows.append(
            {
                "global_position": position,
                "boundary": containing is None,
                "record_id": None if containing is None else containing[2],
                "physical_index": None if containing is None else containing[3],
                "local_offset": None if containing is None else position - containing[0],
                "exact_score": score,
                "literal": window == pattern,
                "exact_accepted": abs(score) <= threshold,
            }
        )
    return rows


def validate_fetch(records, locate, output):
    target = locate["target"]
    if "Dummy" in target:
        if output is not None:
            raise ValueError("dummy target returned a real payload")
        if locate["candidates"]:
            raise ValueError("dummy target has candidates")
        return True
    if "Real" not in target:
        raise ValueError("unknown Fetch target")
    expected_id = target["Real"]["id"]
    physical_index = target["Real"]["physical_index"]
    if not 0 <= physical_index < len(records) or records[physical_index]["id"] != expected_id:
        raise ValueError("real target physical index does not map to RecordId")
    if not locate["candidates"] or locate["candidates"][0]["id"] != expected_id:
        raise ValueError("real target differs from first Locate candidate")
    expected = records[physical_index]
    if output is None or output.get("id") != expected_id:
        raise ValueError("real target returned wrong RecordId")
    if output.get("payload") != expected["payload"]:
        raise ValueError("real target returned wrong payload")
    return True


def decision_disagreements(windows, scores, threshold):
    counts = {
        "ckks_false_positive_valid": 0,
        "ckks_false_negative_valid": 0,
        "ckks_false_positive_boundary": 0,
        "ckks_false_negative_boundary": 0,
    }
    for row in windows:
        approximate = abs(scores[row["global_position"]]) <= threshold
        if approximate == row["exact_accepted"]:
            continue
        location = "boundary" if row["boundary"] else "valid"
        direction = "false_positive" if approximate else "false_negative"
        counts[f"ckks_{direction}_{location}"] += 1
    return counts


def _expected_locate(windows, scores, threshold, dummy_index):
    accepted = [row["global_position"] for row, score in zip(windows, scores) if abs(score) <= threshold]
    rejected = [position for position in accepted if windows[position]["boundary"]]
    candidates = []
    duplicates = 0
    for position in accepted:
        row = windows[position]
        if row["boundary"]:
            continue
        if candidates and candidates[-1]["id"] == row["record_id"]:
            duplicates += 1
            continue
        candidates.append(
            {
                "id": row["record_id"],
                "physical_index": row["physical_index"],
                "global_position": position,
                "local_offset": row["local_offset"],
                "score": scores[position],
            }
        )
    target = (
        {"Real": {"id": candidates[0]["id"], "physical_index": candidates[0]["physical_index"]}}
        if candidates
        else {"Dummy": {"physical_index": dummy_index}}
    )
    return accepted, rejected, candidates, duplicates, target


def _validate_decisions(run, windows, scores, threshold):
    valid = {row["global_position"]: row for row in windows if not row["boundary"]}
    decisions = run["decisions"]
    if len(decisions) != len(valid):
        raise ValueError("native decision count differs from valid-window count")
    for decision in decisions:
        position = decision["global_position"]
        if position not in valid:
            raise ValueError("native decision references a boundary or unknown offset")
        expected = valid[position]
        approximate_accepted = abs(scores[position]) <= threshold
        checks = {
            "id": expected["record_id"],
            "local exact score": expected["exact_score"],
            "semantic match": expected["literal"],
            "exact accepted": expected["exact_accepted"],
            "approximate accepted": approximate_accepted,
        }
        actual = {
            "id": decision["id"],
            "local exact score": decision["exact_score"],
            "semantic match": decision["semantic_match"],
            "exact accepted": decision["exact_accepted"],
            "approximate accepted": decision["approximate_accepted"],
        }
        if actual != checks:
            raise ValueError(f"native decision disagrees with independent oracle at {position}")


def analyze_attempt(records, expected_case, native_row):
    if native_row.get("schema") != RESULT_SCHEMA or native_row.get("case") != expected_case:
        raise ValueError("native result schema or embedded case drift")
    if native_row.get("error") is not None or native_row.get("run") is None:
        return {
            "case_id": expected_case["id"],
            "error": native_row.get("error") or "missing run",
        }
    case = expected_case
    run = native_row["run"]
    oracle = evaluate(
        records,
        case["pattern"],
        case["mode"],
        case["weights"],
        case["threshold"],
    )
    if oracle != case["oracle"]:
        raise ValueError("embedded Python oracle drift")
    windows = independent_windows(
        records, case["pattern"], case["mode"], case["weights"], case["threshold"]
    )
    scores = run["scores"]
    if len(scores) != len(windows) or any(not math.isfinite(score) for score in scores):
        raise ValueError("native score vector length or finiteness failure")
    accepted, rejected, candidates, duplicates, target = _expected_locate(
        windows, scores, case["threshold"], len(records)
    )
    locate = run["locate"]
    if locate["accepted_positions"] != accepted:
        raise ValueError("Locate accepted positions differ from decrypted scores")
    if locate["boundary_rejected_positions"] != rejected:
        raise ValueError("Locate boundary rejections differ from independent spans")
    if locate["candidates"] != candidates:
        raise ValueError("Locate candidates, order, or offsets differ from independent mapping")
    if locate["duplicate_count"] != duplicates:
        raise ValueError("Locate duplicate count differs from independent mapping")
    if locate["truncated_count"] != max(0, len(candidates) - 1) or locate["target"] != target:
        raise ValueError("Locate K=1 truncation or target differs from independent mapping")
    validate_fetch(records, locate, run["output"])
    if run["fetch_count"] != 1:
        raise ValueError("request did not perform exactly one Fetch")
    _validate_decisions(run, windows, scores, case["threshold"])

    valid = [row for row in windows if not row["boundary"]]
    boundary = [row for row in windows if row["boundary"]]
    valid_positions = {row["global_position"] for row in valid}
    approximate_valid = [position for position in accepted if position in valid_positions]
    approximate_boundary = [position for position in accepted if position not in valid_positions]
    exact_valid = [row for row in valid if row["exact_accepted"]]
    exact_boundary = [row for row in boundary if row["exact_accepted"]]
    disagreements = decision_disagreements(windows, scores, case["threshold"])
    approximate_false_accepts = [
        row for row in valid if not row["literal"] and abs(scores[row["global_position"]]) <= case["threshold"]
    ]
    exact_selection = oracle["selected_record_id"]
    semantic_selection = oracle["semantic_selected_record_id"]
    output_id = None if run["output"] is None else run["output"]["id"]
    if run["exact_selection_agreement"] != (output_id == exact_selection):
        raise ValueError("native exact-selection agreement flag drift")
    if run["semantic_selection_agreement"] != (output_id == semantic_selection):
        raise ValueError("native semantic-selection agreement flag drift")
    rust_oracle = run["oracle"]
    if rust_oracle["semantic_ids"] != oracle["literal_record_ids"]:
        raise ValueError("Rust and Python literal RecordId oracles disagree")
    if rust_oracle["exact_ids"] != oracle["valid_candidate_record_ids"]:
        raise ValueError("Rust and Python exact-score RecordId oracles disagree")

    timings = run["timings"]
    return {
        "case_id": case["id"],
        "pattern_id": case["pattern_id"],
        "pattern_kind": case["pattern_kind"],
        "evidence_class": case["evidence_class"],
        "pattern_length": len(case["pattern"]),
        "mode": case["mode"],
        "weight_policy": case["weight_policy"],
        "weight_draw": "" if case["weight_draw"] is None else case["weight_draw"],
        "threshold": case["threshold"],
        "requests": 1,
        "errors": 0,
        "framed_windows": len(windows),
        "valid_windows": len(valid),
        "boundary_windows": len(boundary),
        "nonliteral_valid_windows": sum(not row["literal"] for row in valid),
        "literal_valid_occurrences": sum(row["literal"] for row in valid),
        "literal_record_count": len(oracle["literal_record_ids"]),
        "exact_accept_valid": len(exact_valid),
        "exact_intrinsic_cancellation": sum(
            row["exact_accepted"] and not row["literal"] and row["exact_score"] == 0
            for row in valid
        ),
        "exact_threshold_only": sum(
            row["exact_accepted"] and not row["literal"] and row["exact_score"] != 0
            for row in valid
        ),
        "exact_accept_boundary": len(exact_boundary),
        "approximate_accept_valid": len(approximate_valid),
        "approximate_false_accept_valid": len(approximate_false_accepts),
        "approximate_accept_boundary": len(approximate_boundary),
        **disagreements,
        "locate_boundary_rejects": len(rejected),
        "locate_duplicate_offsets": duplicates,
        "locate_candidate_records": len(candidates),
        "locate_truncated_records": max(0, len(candidates) - 1),
        "selected_real": int(output_id is not None),
        "exact_selection_agreement": int(output_id == exact_selection),
        "semantic_selection_agreement": int(output_id == semantic_selection),
        "fetch_exact": 1,
        "match_ns": timings["match_ns"],
        "locate_ns": timings["locate_ns"],
        "fetch_ns": timings["fetch_ns"],
        "request_ns": timings["request_ns"],
        "derived_online_sum_ns": timings["derived_online_sum_ns"],
        "fetch_query_ns": timings["fetch"]["query_ns"],
        "fetch_server_ns": timings["fetch"]["server_ns"],
        "fetch_decrypt_decode_ns": timings["fetch"]["decrypt_decode_ns"],
        "error": "",
    }


COUNT_FIELDS = (
    "requests", "errors", "framed_windows", "valid_windows", "boundary_windows",
    "nonliteral_valid_windows", "literal_valid_occurrences", "literal_record_count",
    "exact_accept_valid", "exact_intrinsic_cancellation", "exact_threshold_only",
    "exact_accept_boundary", "approximate_accept_valid", "approximate_false_accept_valid",
    "approximate_accept_boundary", "ckks_false_positive_valid", "ckks_false_negative_valid",
    "ckks_false_positive_boundary", "ckks_false_negative_boundary",
    "locate_boundary_rejects", "locate_duplicate_offsets", "locate_candidate_records",
    "locate_truncated_records", "selected_real", "exact_selection_agreement",
    "semantic_selection_agreement", "fetch_exact",
)
TIMING_FIELDS = ("match_ns", "locate_ns", "fetch_ns", "request_ns", "derived_online_sum_ns")


def _percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, math.ceil(fraction * len(ordered)) - 1)]


def aggregate(attempts):
    groups = defaultdict(list)
    for row in attempts:
        key = (
            row.get("evidence_class", "error"), row.get("pattern_kind", "error"),
            row.get("mode", "error"), row.get("weight_policy", "error"),
            row.get("threshold", ""),
        )
        groups[key].append(row)
    cells = []
    for key, rows in sorted(groups.items(), key=lambda item: tuple(map(str, item[0]))):
        cell = dict(zip(
            ("evidence_class", "pattern_kind", "mode", "weight_policy", "threshold"), key
        ))
        for field in COUNT_FIELDS:
            cell[field] = sum(int(row.get(field, 0)) for row in rows)
        good = [row for row in rows if not row.get("error")]
        for field in TIMING_FIELDS:
            values = [int(row[field]) for row in good]
            cell[f"{field}_median"] = statistics.median(values) if values else None
            cell[f"{field}_p95"] = _percentile(values, 0.95) if values else None
        cells.append(cell)
    return cells


def _multiplicity(records, patterns):
    rows = []
    for pattern in patterns:
        if pattern["evidence_class"] != "natural":
            continue
        value = bytes(pattern["pattern"])
        per_record = {}
        for record in records:
            searchable = bytes(record["searchable"])
            count = sum(
                searchable[offset : offset + len(value)] == value
                for offset in range(max(0, len(searchable) - len(value) + 1))
            )
            if count:
                per_record[record["id"]] = count
        total = sum(per_record.values())
        rows.append(
            {
                "pattern_id": pattern["id"],
                "pattern_kind": pattern["kind"],
                "length": len(value),
                "literal_occurrences": total,
                "distinct_record_ids": len(per_record),
                "within_record_duplicate_offsets": sum(max(0, count - 1) for count in per_record.values()),
                "record_occurrence_counts": json.dumps(per_record, sort_keys=True),
            }
        )
    return rows


def _write_csv(path, rows):
    if not rows:
        raise ValueError(f"cannot write empty CSV: {path}")
    with path.open("x", newline="") as destination:
        writer = csv.DictWriter(destination, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def analyze(catalog_path, patterns_path, cases_path, native_path, output_directory):
    catalog = json.loads(Path(catalog_path).read_text())
    patterns_doc = json.loads(Path(patterns_path).read_text())
    if catalog.get("schema") != SCHEMA or patterns_doc.get("schema") != SCHEMA:
        raise ValueError("catalog or pattern schema drift")
    records = catalog["records"]
    expected_cases = {}
    with Path(cases_path).open() as source:
        for line in source:
            case = json.loads(line)
            if case["id"] in expected_cases:
                raise ValueError("duplicate expected case ID")
            expected_cases[case["id"]] = case
    if len(expected_cases) != 643:
        raise ValueError(f"expected 643 cases, got {len(expected_cases)}")

    attempts = []
    seen = set()
    with Path(native_path).open() as source:
        first = next(source, None)
        if first is None:
            raise ValueError("empty native output")
        metadata = json.loads(first)
        if metadata.get("schema") != METADATA_SCHEMA or metadata.get("attempts") != 643:
            raise ValueError("native metadata drift")
        for line_number, line in enumerate(source, 2):
            native = json.loads(line)
            case_id = native.get("case", {}).get("id")
            if case_id not in expected_cases or case_id in seen:
                raise ValueError(f"unknown or duplicate native case at line {line_number}")
            seen.add(case_id)
            attempts.append(analyze_attempt(records, expected_cases[case_id], native))
    missing = set(expected_cases) - seen
    if missing:
        raise ValueError(f"missing {len(missing)} native cases")

    output = Path(output_directory)
    output.mkdir(parents=True, exist_ok=True)
    cells = aggregate(attempts)
    multiplicity = _multiplicity(records, patterns_doc["patterns"])
    good = [row for row in attempts if not row.get("error")]
    natural_rfps = [
        row for row in good
        if row["mode"] == "rfps" and row["weight_policy"] == "uniform_3_32"
        and row["evidence_class"] == "natural"
    ]
    headline = {
        "attempts": len(attempts),
        "completed_attempts": len(good),
        "execution_errors": len(attempts) - len(good),
        "selected_real_records": sum(row["selected_real"] for row in good),
        "selected_real_records_reconstructed_exactly": sum(
            row["fetch_exact"] and row["selected_real"] for row in good
        ),
        "dummy_fetches": sum(not row["selected_real"] for row in good),
        "all_fetch_attempts_exact": sum(row["fetch_exact"] for row in good),
        "natural_uniform_rfps": {
            "unit": "window-decision; repeated weight/threshold attempts remain separate",
            "requests": len(natural_rfps),
            "nonliteral_valid_windows": sum(row["nonliteral_valid_windows"] for row in natural_rfps),
            "intrinsic_cancellations": sum(row["exact_intrinsic_cancellation"] for row in natural_rfps),
            "threshold_only_acceptances": sum(row["exact_threshold_only"] for row in natural_rfps),
            "ckks_false_positive_decisions": sum(row["ckks_false_positive_valid"] for row in natural_rfps),
            "ckks_false_negative_decisions": sum(row["ckks_false_negative_valid"] for row in natural_rfps),
        },
        "boundary": {
            "unit": "boundary-window decision",
            "examined": sum(row["boundary_windows"] for row in good),
            "exact_accepted": sum(row["exact_accept_boundary"] for row in good),
            "ckks_accepted": sum(row["approximate_accept_boundary"] for row in good),
            "ckks_false_positive": sum(row["ckks_false_positive_boundary"] for row in good),
            "ckks_false_negative": sum(row["ckks_false_negative_boundary"] for row in good),
            "locate_rejected": sum(row["locate_boundary_rejects"] for row in good),
        },
        "deduplication": {
            "unit": "request or extra accepted offset as named",
            "requests_where_locate_deduplicated": sum(row["locate_duplicate_offsets"] > 0 for row in good),
            "accepted_offsets_removed": sum(row["locate_duplicate_offsets"] for row in good),
            "natural_patterns_with_within_record_duplicates": sum(
                row["within_record_duplicate_offsets"] > 0 for row in multiplicity
            ),
            "natural_within_record_duplicate_offsets": sum(
                row["within_record_duplicate_offsets"] for row in multiplicity
            ),
        },
        "selection_agreement": {
            "exact_score": sum(row["exact_selection_agreement"] for row in good),
            "literal_semantics": sum(row["semantic_selection_agreement"] for row in good),
            "denominator": len(good),
        },
    }
    summary = {
        "schema": "p0d-bgl-k1-analysis-v1",
        "classification": "bounded realistic K=1 record-level workload",
        "catalog": {
            "records": len(records),
            "selected_record_ids": catalog["selected_record_ids"],
            "payload_bytes": catalog["payload_bytes"],
            "framed_search_bytes": catalog["framed_search_bytes"],
        },
        "native_metadata": metadata,
        "headline": headline,
        "cells": cells,
        "limits": [
            "Rates are bounded BGL-suffix observations, not global RFPS probabilities",
            "Synthetic and realistic rates are not pooled",
            "Thresholds are distinct semantic acceptance predicates",
            "Exact Fetch does not repair a semantically false Match selection",
            "The selected payload is a BGL message suffix, not a full source line",
            "No RFPS correction, security proof, or deployment claim is made",
        ],
    }
    _write_csv(output / "attempts.csv", attempts)
    _write_csv(output / "summary.csv", cells)
    _write_csv(output / "multiplicity.csv", multiplicity)
    with (output / "summary.json").open("x") as destination:
        json.dump(summary, destination, indent=2)
        destination.write("\n")
    return summary


if __name__ == "__main__":
    if len(sys.argv) != 6:
        raise SystemExit(
            "usage: analysis.py CATALOG_JSON PATTERNS_JSON CASES_JSONL NATIVE_JSONL OUTPUT_DIRECTORY"
        )
    report = analyze(*(Path(argument) for argument in sys.argv[1:]))
    print(json.dumps(report["headline"], indent=2))
