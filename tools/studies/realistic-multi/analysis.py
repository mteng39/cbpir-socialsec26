"""Validate K=4 BGL output against the frozen independent plaintext oracle."""

from collections import Counter, defaultdict
import csv
import json
import math
from pathlib import Path
import statistics
import sys


RESULT_SCHEMA = "p0d-bgl-kgt1-native-result-v1"
METADATA_SCHEMA = "p0d-bgl-kgt1-native-metadata-v1"
PUBLIC_K = 4


def load_jsonl(path):
    with Path(path).open() as source:
        return [json.loads(line) for line in source if line.strip()]


def _score(window, pattern, mode, weights):
    if mode == "fps":
        return sum((left - right) ** 2 for left, right in zip(window, pattern))
    return sum(
        (left - right) * weight
        for left, right, weight in zip(window, pattern, weights)
    )


def independent_windows(records, case):
    pattern = bytes(case["pattern"])
    framed = bytearray()
    spans = []
    for physical_index, record in enumerate(records):
        start = len(framed)
        framed.extend(record["searchable"])
        spans.append((start, len(framed), record["id"], physical_index))
        if physical_index + 1 < len(records):
            framed.append(0)
    rows = []
    for position in range(len(framed) - len(pattern) + 1):
        end = position + len(pattern)
        span = next(
            (span for span in spans if span[0] <= position and end <= span[1]),
            None,
        )
        window = bytes(framed[position:end])
        score = _score(window, pattern, case["mode"], case["weights"])
        rows.append({
            "global_position": position,
            "boundary": span is None,
            "record_id": None if span is None else span[2],
            "physical_index": None if span is None else span[3],
            "local_offset": None if span is None else position - span[0],
            "exact_score": score,
            "literal": window == pattern,
            "exact_accepted": abs(score) <= case["threshold"],
        })
    return rows


def expected_locate(windows, scores, threshold):
    accepted = [
        row["global_position"]
        for row, score in zip(windows, scores)
        if abs(score) <= threshold
    ]
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
        candidates.append({
            "id": row["record_id"],
            "physical_index": row["physical_index"],
            "global_position": position,
            "local_offset": row["local_offset"],
            "score": scores[position],
        })
    candidates.sort(key=lambda candidate: (candidate["id"], candidate["global_position"]))
    return accepted, rejected, candidates, candidates[:PUBLIC_K], duplicates


def _validate_decisions(run, windows, scores, threshold):
    valid = {row["global_position"]: row for row in windows if not row["boundary"]}
    if len(run["decisions"]) != len(valid):
        raise ValueError("native decision count differs from valid-window count")
    for decision in run["decisions"]:
        row = valid.get(decision["global_position"])
        if row is None:
            raise ValueError("native decision references boundary or unknown position")
        expected = {
            "id": row["record_id"],
            "exact_score": row["exact_score"],
            "semantic_match": row["literal"],
            "exact_accepted": row["exact_accepted"],
            "approximate_accepted": abs(scores[row["global_position"]]) <= threshold,
        }
        actual = {key: decision[key] for key in expected}
        if actual != expected:
            raise ValueError(f"decision drift at position {row['global_position']}")


def _disagreements(windows, scores, threshold):
    result = Counter()
    for row in windows:
        approximate = abs(scores[row["global_position"]]) <= threshold
        if approximate == row["exact_accepted"]:
            continue
        location = "boundary" if row["boundary"] else "valid"
        direction = "false_positive" if approximate else "false_negative"
        result[f"ckks_{direction}_{location}"] += 1
    return result


def _validate_fetch(records, selected, run):
    outputs = run["outputs"]
    expected_ids = [candidate["id"] for candidate in selected]
    expected_physical = [candidate["physical_index"] for candidate in selected]
    if [output["id"] for output in outputs] != expected_ids:
        raise ValueError("Fetch outputs differ from client-selected stable IDs")
    fetch = run["fetch"]
    checks = {
        "selected_physical_indices": expected_physical,
        "selected_real_count": len(selected),
        "dummy_count": PUBLIC_K - len(selected),
        "unselected_windows_zero": True,
    }
    for name, value in checks.items():
        if fetch[name] != value:
            raise ValueError(f"Fetch trace mismatch: {name}")
    if fetch["dummy_flags"] != [False] * len(selected) + [True] * (PUBLIC_K - len(selected)):
        raise ValueError("P3 dummy flags differ from selected width")
    if fetch["planned_physical_indices"][:len(selected)] != expected_physical:
        raise ValueError("P3 plan real prefix differs from selected physical rows")
    if len(fetch["planned_physical_indices"]) != PUBLIC_K:
        raise ValueError("P3 plan does not have public K entries")
    for output, candidate in zip(outputs, selected):
        source = records[candidate["physical_index"]]
        if source["id"] != output["id"] or source["payload"] != output["payload"]:
            raise ValueError("Fetch reconstructed the wrong stable ID or payload")
    if run["fetch_count"] != 1:
        raise ValueError("request did not perform exactly one Fetch")


def _message_sizes(ckks, p3):
    p3_case = next(case for case in p3["cases"] if case["public_k"] == PUBLIC_K)
    p3_sizes = p3_case["serialization_bytes"]
    result = {
        "classification": (
            "derived from separate randomized serialization measurements; "
            "online ciphertexts only; contexts and keys excluded"
        ),
        "p3": {
            "public_k": PUBLIC_K,
            "client_to_server_bytes": p3_sizes["query"],
            "server_to_client_bytes": p3_sizes["response"],
            "query_ciphertexts": 1,
            "response_ciphertexts": 1,
            "evaluation_key_bytes": p3_sizes["evaluation_key"],
        },
    }
    for protocol in ("fps", "rfps"):
        probes = [
            probe for probe in ckks["probes"]
            if probe.get("protocol", probe.get("mode")) == protocol
        ]
        if not probes:
            raise ValueError(f"missing {protocol} CKKS size probe")
        shapes = {
            (tuple(probe["query_ciphertext_bytes"]), probe["response_ciphertext_bytes"])
            for probe in probes
        }
        if len(shapes) != 1:
            raise ValueError(f"{protocol} CKKS ciphertext sizes differ across probes")
        probe = probes[0]
        match_up = sum(probe["query_ciphertext_bytes"])
        match_down = probe["response_ciphertext_bytes"]
        result[protocol] = {
            "match_client_to_server_bytes": match_up,
            "match_server_to_client_bytes": match_down,
            "fetch_client_to_server_bytes": p3_sizes["query"],
            "fetch_server_to_client_bytes": p3_sizes["response"],
            "client_to_server_bytes": match_up + p3_sizes["query"],
            "server_to_client_bytes": match_down + p3_sizes["response"],
            "sequential_messages": "one Match exchange followed by one Fetch exchange",
        }
    return result


def analyze_attempt(records, case, native):
    if native.get("schema") != RESULT_SCHEMA or native.get("case") != case:
        raise ValueError("native result schema or embedded case drift")
    if native.get("error") is not None or native.get("run") is None:
        raise ValueError(f"native execution error: {native.get('error')}")
    run = native["run"]
    if run["public_k"] != PUBLIC_K or run["ckks_metrics"]["openfhe_version"] != "1.5.1":
        raise ValueError("public K or OpenFHE runtime drift")
    windows = independent_windows(records, case)
    scores = run["scores"]
    if len(scores) != len(windows) or any(not math.isfinite(score) for score in scores):
        raise ValueError("native score vector length or finiteness failure")
    accepted, rejected, candidates, selected, duplicates = expected_locate(
        windows, scores, case["threshold"]
    )
    locate = run["locate"]
    checks = {
        "public_k": PUBLIC_K,
        "accepted_positions": accepted,
        "boundary_rejected_positions": rejected,
        "candidates": candidates,
        "selected_candidates": selected,
        "duplicate_count": duplicates,
        "truncated_count": max(0, len(candidates) - PUBLIC_K),
    }
    for name, value in checks.items():
        if locate[name] != value:
            raise ValueError(f"Locate mismatch: {name}")
    _validate_decisions(run, windows, scores, case["threshold"])
    _validate_fetch(records, selected, run)

    oracle = case["oracle"]
    rust_oracle = run["oracle"]
    if rust_oracle["semantic_ids"] != oracle["literal_record_ids"]:
        raise ValueError("Rust and frozen literal RecordId oracles disagree")
    if rust_oracle["exact_ids"] != oracle["valid_candidate_record_ids"]:
        raise ValueError("Rust and frozen exact-score RecordId oracles disagree")
    oracle_selected = oracle["valid_candidate_record_ids"][:PUBLIC_K]
    semantic_selected = oracle["literal_record_ids"][:PUBLIC_K]
    output_ids = [output["id"] for output in run["outputs"]]
    if run["exact_selection_agreement"] != (output_ids == oracle_selected):
        raise ValueError("exact-selection agreement flag drift")
    if run["semantic_selection_agreement"] != (output_ids == semantic_selected):
        raise ValueError("semantic-selection agreement flag drift")

    disagreements = _disagreements(windows, scores, case["threshold"])
    literal_ids = set(oracle["literal_record_ids"])
    physical = [candidate["physical_index"] for candidate in selected]
    adjacent = any(right - left == 1 for left, right in zip(physical, physical[1:]))
    nonadjacent = any(right - left > 1 for left, right in zip(physical, physical[1:]))
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
        "valid_windows": oracle["valid_windows"],
        "boundary_windows": oracle["boundary_windows"],
        "literal_record_count": len(oracle["literal_record_ids"]),
        "oracle_candidate_count": len(oracle["valid_candidate_record_ids"]),
        "candidate_count": len(candidates),
        "selected_count": len(selected),
        "false_selected_count": sum(candidate["id"] not in literal_ids for candidate in selected),
        "duplicate_count": duplicates,
        "oracle_duplicate_count": oracle["duplicate_candidate_offsets"],
        "truncated_count": max(0, len(candidates) - PUBLIC_K),
        "oracle_truncated_count": max(0, len(oracle["valid_candidate_record_ids"]) - PUBLIC_K),
        "boundary_rejected_count": len(rejected),
        "oracle_boundary_accept_count": len(oracle["exact_accepted_boundary_positions"]),
        "intrinsic_cancellation_count": len(oracle["intrinsic_cancellation_positions"]),
        "threshold_only_count": len(oracle["threshold_only_positions"]),
        "ckks_false_positive_valid": disagreements["ckks_false_positive_valid"],
        "ckks_false_negative_valid": disagreements["ckks_false_negative_valid"],
        "ckks_false_positive_boundary": disagreements["ckks_false_positive_boundary"],
        "ckks_false_negative_boundary": disagreements["ckks_false_negative_boundary"],
        "exact_selection_agreement": run["exact_selection_agreement"],
        "semantic_selection_agreement": run["semantic_selection_agreement"],
        "selected_has_adjacent_pair": adjacent,
        "selected_has_nonadjacent_pair": nonadjacent,
        "fetch_payloads_exact": len(run["outputs"]),
        "fetch_payloads_inexact": 0,
        "unselected_windows_zero": run["fetch"]["unselected_windows_zero"],
        "match_ns": run["timings"]["match_ns"],
        "locate_ns": run["timings"]["locate_ns"],
        "fetch_ns": run["timings"]["fetch_ns"],
        "request_ns": run["timings"]["request_ns"],
    }


def _aggregate(attempts):
    sum_field = lambda name: sum(row[name] for row in attempts)
    count_if = lambda predicate: sum(bool(predicate(row)) for row in attempts)
    summary = {
        "requests": len(attempts),
        "execution_errors": 0,
        "public_k": PUBLIC_K,
        "oracle_literal_multi_record_requests": count_if(lambda row: row["literal_record_count"] > 1),
        "oracle_candidate_size_max": max(row["oracle_candidate_count"] for row in attempts),
        "native_candidate_size_max": max(row["candidate_count"] for row in attempts),
        "oracle_over_k_requests": count_if(lambda row: row["oracle_candidate_count"] > PUBLIC_K),
        "native_over_k_requests": count_if(lambda row: row["candidate_count"] > PUBLIC_K),
        "oracle_dedup_requests": count_if(lambda row: row["oracle_duplicate_count"] > 0),
        "native_dedup_requests": count_if(lambda row: row["duplicate_count"] > 0),
        "oracle_duplicate_positions": sum_field("oracle_duplicate_count"),
        "native_duplicate_positions": sum_field("duplicate_count"),
        "oracle_boundary_accept_requests": count_if(lambda row: row["oracle_boundary_accept_count"] > 0),
        "native_boundary_reject_requests": count_if(lambda row: row["boundary_rejected_count"] > 0),
        "oracle_boundary_accept_positions": sum_field("oracle_boundary_accept_count"),
        "native_boundary_rejected_positions": sum_field("boundary_rejected_count"),
        "selected_records": sum_field("selected_count"),
        "false_selected_records": sum_field("false_selected_count"),
        "requests_with_false_selected_record": count_if(lambda row: row["false_selected_count"] > 0),
        "fetch_payloads_exact": sum_field("fetch_payloads_exact"),
        "fetch_payloads_inexact": sum_field("fetch_payloads_inexact"),
        "nonzero_unselected_window_requests": count_if(lambda row: not row["unselected_windows_zero"]),
        "selected_adjacent_pair_requests": count_if(lambda row: row["selected_has_adjacent_pair"]),
        "selected_nonadjacent_pair_requests": count_if(lambda row: row["selected_has_nonadjacent_pair"]),
        "intrinsic_cancellation_positions": sum_field("intrinsic_cancellation_count"),
        "threshold_only_positions": sum_field("threshold_only_count"),
        "ckks_false_positive_valid": sum_field("ckks_false_positive_valid"),
        "ckks_false_negative_valid": sum_field("ckks_false_negative_valid"),
        "ckks_false_positive_boundary": sum_field("ckks_false_positive_boundary"),
        "ckks_false_negative_boundary": sum_field("ckks_false_negative_boundary"),
        "valid_decision_windows": sum_field("valid_windows"),
        "boundary_windows": sum_field("boundary_windows"),
        "stage_median_ns": {
            stage: int(statistics.median(row[f"{stage}_ns"] for row in attempts))
            for stage in ("match", "locate", "fetch", "request")
        },
        "counting_units": {
            "requests": "one frozen pattern/mode/weight/threshold request",
            "candidate_distribution": "distinct native candidate RecordIds per request",
            "selected_and_false_selected": "selected RecordId",
            "dedup_and_boundary": "request counts and positions are separate fields",
            "ckks_disagreements": "valid within-record or boundary window as named",
            "fetch": "selected record; unselected isolation is per request",
        },
        "rates_must_not_be_pooled_with_synthetic": True,
    }
    if summary["fetch_payloads_inexact"] or summary["nonzero_unselected_window_requests"]:
        raise ValueError("Fetch correctness gate failed")
    if summary["fetch_payloads_exact"] != summary["selected_records"]:
        raise ValueError("selected and exactly reconstructed record counts differ")
    return summary


def _distribution(attempts):
    counts = Counter(row["candidate_count"] for row in attempts)
    return [
        {"candidate_size": size, "requests": counts[size]}
        for size in sorted(counts)
    ]


def _rfps(attempts):
    groups = defaultdict(list)
    for row in attempts:
        if row["mode"] == "rfps":
            groups[(row["weight_policy"], row["threshold"])].append(row)
    result = []
    for (policy, threshold), rows in sorted(groups.items()):
        result.append({
            "weight_policy": policy,
            "threshold": threshold,
            "requests": len(rows),
            "selected_records": sum(row["selected_count"] for row in rows),
            "false_selected_records": sum(row["false_selected_count"] for row in rows),
            "requests_with_false_selected_record": sum(row["false_selected_count"] > 0 for row in rows),
            "intrinsic_cancellation_positions": sum(row["intrinsic_cancellation_count"] for row in rows),
            "threshold_only_positions": sum(row["threshold_only_count"] for row in rows),
            "ckks_decision_disagreements": sum(
                row["ckks_false_positive_valid"] + row["ckks_false_negative_valid"]
                + row["ckks_false_positive_boundary"] + row["ckks_false_negative_boundary"]
                for row in rows
            ),
        })
    return result


def analyze(catalog, cases, native_rows, ckks, p3):
    records = catalog["records"]
    if catalog["selected_record_ids"] != [record["id"] for record in records]:
        raise ValueError("catalog RecordId order drift")
    if len(cases) != 643 or len(native_rows) != 644:
        raise ValueError("realistic request or result count drift")
    metadata = native_rows[0]
    if metadata.get("schema") != METADATA_SCHEMA or metadata.get("attempts") != 643:
        raise ValueError("native metadata drift")
    if metadata.get("public_k") != PUBLIC_K:
        raise ValueError("native metadata public K drift")
    attempts = [
        analyze_attempt(records, case, native)
        for case, native in zip(cases, native_rows[1:])
    ]
    timings = [
        {key: row[key] for key in ("case_id", "match_ns", "locate_ns", "fetch_ns", "request_ns")}
        for row in attempts
    ]
    return {
        "summary": _aggregate(attempts),
        "messages": _message_sizes(ckks, p3),
        "attempts": attempts,
        "candidate_distribution": _distribution(attempts),
        "timings": timings,
        "rfps_propagation": _rfps(attempts),
    }


def _write_json(path, value):
    if path.exists():
        raise FileExistsError(path)
    path.write_text(json.dumps(value, indent=2) + "\n")


def _write_csv(path, rows):
    if path.exists():
        raise FileExistsError(path)
    if not rows:
        raise ValueError(f"no rows for {path.name}")
    with path.open("x", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def write_outputs(directory, result):
    directory.mkdir(parents=True, exist_ok=True)
    paths = [directory / name for name in (
        "summary.json", "messages.json", "attempts.csv",
        "candidate_distribution.csv", "timings.csv", "rfps_propagation.csv",
    )]
    _write_json(paths[0], result["summary"])
    _write_json(paths[1], result["messages"])
    _write_csv(paths[2], result["attempts"])
    _write_csv(paths[3], result["candidate_distribution"])
    _write_csv(paths[4], result["timings"])
    _write_csv(paths[5], result["rfps_propagation"])
    return paths


def main(arguments):
    if len(arguments) != 7:
        raise SystemExit(
            "usage: analysis.py CATALOG CASES_JSONL NATIVE_JSONL CKKS_SIZES P3_REPORT OUTPUT_DIR"
        )
    catalog = json.loads(Path(arguments[1]).read_text())
    cases = load_jsonl(arguments[2])
    native = load_jsonl(arguments[3])
    ckks = json.loads(Path(arguments[4]).read_text())
    p3 = json.loads(Path(arguments[5]).read_text())
    result = analyze(catalog, cases, native, ckks, p3)
    for path in write_outputs(Path(arguments[6]), result):
        print(path)


if __name__ == "__main__":
    main(sys.argv)


def _write_json(path, value):
    if path.exists():
        raise FileExistsError(path)
    path.write_text(json.dumps(value, indent=2) + "\n")


def _write_csv(path, rows):
    if path.exists():
        raise FileExistsError(path)
    with path.open("x", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def write_outputs(directory, result):
    directory.mkdir(parents=True, exist_ok=True)
    paths = [directory / name for name in (
        "summary.json", "messages.json", "attempts.csv",
        "candidate_distribution.csv", "timings.csv", "rfps_propagation.csv",
    )]
    _write_json(paths[0], result["summary"])
    _write_json(paths[1], result["messages"])
    _write_csv(paths[2], result["attempts"])
    _write_csv(paths[3], result["candidate_distribution"])
    _write_csv(paths[4], result["timings"])
    _write_csv(paths[5], result["rfps_propagation"])
    return paths


def main(arguments):
    if len(arguments) != 7:
        raise SystemExit(
            "usage: analysis.py CATALOG CASES_JSONL NATIVE_JSONL CKKS_SIZES P3_REPORT OUTPUT_DIR"
        )
    catalog = json.loads(Path(arguments[1]).read_text())
    cases = load_jsonl(arguments[2])
    native = load_jsonl(arguments[3])
    ckks = json.loads(Path(arguments[4]).read_text())
    p3 = json.loads(Path(arguments[5]).read_text())
    result = analyze(catalog, cases, native, ckks, p3)
    for path in write_outputs(Path(arguments[6]), result):
        print(path)


if __name__ == "__main__":
    main(sys.argv)
