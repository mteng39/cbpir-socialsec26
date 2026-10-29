"""Validate and summarize the frozen Phase H report without changing rates."""

import csv
import json
from pathlib import Path
import statistics
import sys


CASE_NAMES = [
    "zero_matches", "one_record", "fewer_than_k", "exactly_k",
    "more_than_k", "one_record_many_occurrences",
    "duplicates_across_records", "adjacent_records",
    "nonadjacent_records", "first_and_last_records", "cross_record",
    "rfps_false_only_cancellation", "rfps_false_competes",
    "rfps_threshold_only",
]


def _ids(records):
    return [record["id"] for record in records]


def _probe(ckks, protocol):
    return next(item for item in ckks["probes"] if item["protocol"] == protocol)


def _messages(ckks, p3):
    p3_case = next(item for item in p3["cases"] if item["public_k"] == 4)
    p3_sizes = p3_case["serialization_bytes"]
    result = {
        "classification": (
            "derived from separate randomized serialization measurements; "
            "online ciphertexts only; contexts and keys excluded"
        ),
        "p3": {
            "public_k": 4,
            "client_to_server_bytes": p3_sizes["query"],
            "server_to_client_bytes": p3_sizes["response"],
            "query_ciphertexts": 1,
            "response_ciphertexts": 1,
            "evaluation_key_bytes": p3_sizes["evaluation_key"],
        },
    }
    for protocol in ("fps", "rfps"):
        probe = _probe(ckks, protocol)
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


def analyze(report, ckks, p3):
    assert report["schema"] == "p0d-kgt1-synthetic-v1"
    assert report["bounded_fixture_gate_passed"] is True
    assert report["public_k"] == 4
    assert report["case_count"] == len(CASE_NAMES)
    assert report["errors"] == []
    repetitions = report["measured_repetitions_per_case"]
    assert repetitions > 0
    assert [case["fixture"]["name"] for case in report["cases"]] == CASE_NAMES

    cases = []
    timings = []
    totals = {
        "measured_requests": 0,
        "execution_errors": 0,
        "fetch_requests": 0,
        "selected_records": 0,
        "exactly_reconstructed_records": 0,
        "fetch_payloads_inexact": 0,
        "false_selected_records": 0,
        "boundary_rejected_positions": 0,
        "deduplicated_positions": 0,
        "truncated_candidates": 0,
        "capacity_exceeded_requests": 0,
        "intrinsic_cancellation_windows": 0,
        "threshold_only_windows": 0,
        "ckks_false_positive_windows": 0,
        "ckks_false_negative_windows": 0,
        "decision_windows": 0,
    }

    for case in report["cases"]:
        fixture = case["fixture"]
        assert case["all_expected"] is True
        assert case["warmup_count"] == 1
        assert case["warmup_excluded_from_statistics"] is True
        assert case["measured_repetition_count"] == repetitions
        assert len(case["repetitions"]) == repetitions
        expected_selected = fixture["expected_selected_ids"]
        semantic = set(fixture["expected_semantic_ids"])

        first = case["repetitions"][0]
        cases.append({
            "case": fixture["name"],
            "candidate_count": len(first["locate"]["candidates"]),
            "selected_count": len(first["locate"]["selected_candidates"]),
            "truncated_count": first["locate"]["truncated_count"],
            "false_selected_count": sum(
                output["id"] not in semantic for output in first["outputs"]
            ),
            "boundary_rejected_count": len(first["locate"]["boundary_rejected_positions"]),
            "duplicate_count": first["locate"]["duplicate_count"],
        })

        source_payloads = {record["id"]: record["payload"] for record in fixture["records"]}
        for index, run in enumerate(case["repetitions"], start=1):
            totals["measured_requests"] += 1
            totals["fetch_requests"] += run["fetch_count"]
            assert run["fetch_count"] == 1
            assert run["public_k"] == 4
            assert _ids(run["outputs"]) == expected_selected
            assert run["fetch"]["selected_real_count"] == len(expected_selected)
            assert run["fetch"]["dummy_count"] == 4 - len(expected_selected)
            assert run["fetch"]["unselected_windows_zero"] is True
            totals["selected_records"] += len(run["outputs"])
            for output in run["outputs"]:
                if output["payload"] == source_payloads[output["id"]]:
                    totals["exactly_reconstructed_records"] += 1
                else:
                    totals["fetch_payloads_inexact"] += 1
                totals["false_selected_records"] += output["id"] not in semantic
            totals["boundary_rejected_positions"] += len(
                run["locate"]["boundary_rejected_positions"]
            )
            totals["deduplicated_positions"] += run["locate"]["duplicate_count"]
            totals["truncated_candidates"] += run["locate"]["truncated_count"]
            totals["capacity_exceeded_requests"] += run["locate"]["truncated_count"] > 0
            for decision in run["decisions"]:
                totals["decision_windows"] += 1
                totals["intrinsic_cancellation_windows"] += decision["intrinsic_cancellation"]
                totals["threshold_only_windows"] += decision["threshold_only"]
                totals["ckks_false_positive_windows"] += decision["ckks_false_positive"]
                totals["ckks_false_negative_windows"] += decision["ckks_false_negative"]
            timings.append({
                "case": fixture["name"],
                "repetition": index,
                "match_ns": run["timings"]["match_ns"],
                "locate_ns": run["timings"]["locate_ns"],
                "fetch_ns": run["timings"]["fetch_ns"],
                "request_ns": run["timings"]["request_ns"],
            })

    totals["ckks_decision_disagreements"] = (
        totals["ckks_false_positive_windows"] + totals["ckks_false_negative_windows"]
    )
    assert totals["fetch_payloads_inexact"] == 0
    assert totals["exactly_reconstructed_records"] == totals["selected_records"]
    totals["counting_units"] = {
        "measured_requests": "request; warmups excluded",
        "selected_and_reconstructed": "selected record",
        "false_selected_records": "selected record absent from literal-match RecordIds",
        "boundary_and_duplicate_and_truncated": "position or candidate as named",
        "match_effects": "valid per-record decision window; guard-crossing windows excluded",
    }
    totals["stage_median_ns_across_requests"] = {
        stage: int(statistics.median(row[f"{stage}_ns"] for row in timings))
        for stage in ("match", "locate", "fetch", "request")
    }
    return {
        "summary": totals,
        "messages": _messages(ckks, p3),
        "cases": cases,
        "timings": timings,
    }


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
        "summary.json", "messages.json", "cases.csv", "timings.csv"
    )]
    for path, value in zip(paths[:2], (result["summary"], result["messages"])):
        if path.exists():
            raise FileExistsError(path)
        path.write_text(json.dumps(value, indent=2) + "\n")
    _write_csv(paths[2], result["cases"])
    _write_csv(paths[3], result["timings"])
    return paths


def main(arguments):
    if len(arguments) != 5:
        raise SystemExit("usage: analysis.py REPORT CKKS_SIZES P3_REPORT OUTPUT_DIR")
    report, ckks, p3 = (json.loads(Path(path).read_text()) for path in arguments[1:4])
    for path in write_outputs(Path(arguments[4]), analyze(report, ckks, p3)):
        print(path)


if __name__ == "__main__":
    main(sys.argv)
