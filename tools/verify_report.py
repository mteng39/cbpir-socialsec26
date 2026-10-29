#!/usr/bin/env python3
"""Recompute the bounded P0-D report contract from retained logical inputs."""
import json
import math
import statistics
import sys
from pathlib import Path

FIXTURES = {
    "exact_first", "exact_last", "near_mismatch", "no_match", "ties",
    "duplicates", "cross_record", "zero_inside_record", "rfps_exact",
    "rfps_cancellation", "rfps_threshold_only",
}
PROBES = {"fps": "exact_first", "rfps": "rfps_exact"}
EFFECTS = (
    "intrinsic_cancellation", "threshold_only",
    "ckks_false_positive", "ckks_false_negative",
)


def check_case(fixture, run, catalog=None):
    """Validate one observed run, including warmups and performance repeats."""
    records = sorted(fixture["records"], key=lambda r: r["id"])
    request = fixture["request"]
    pattern = request["pattern"]
    threshold = request["threshold"]
    text, spans = [], []
    for index, record in enumerate(records):
        if index:
            text.append(0)
        start = len(text)
        text.extend(record["searchable"])
        spans.append({
            "id": record["id"], "physical_index": index,
            "start": start, "end": len(text),
        })
    if catalog is not None:
        assert catalog == {"records": records, "text": text, "spans": spans}
    scores = run["scores"]
    assert len(scores) == len(text) - len(pattern) + 1
    assert all(math.isfinite(score) for score in scores)
    positions = [i for i, score in enumerate(scores) if abs(score) <= threshold]
    candidates, boundaries, duplicates = [], [], 0
    for position in positions:
        span = next((s for s in spans if s["start"] <= position
                     and position + len(pattern) <= s["end"]), None)
        if span is None:
            boundaries.append(position)
        elif candidates and candidates[-1]["id"] == span["id"]:
            duplicates += 1
        else:
            candidates.append({
                "id": span["id"], "physical_index": span["physical_index"],
                "global_position": position, "local_offset": position-span["start"],
                "score": scores[position],
            })
    target = ({"Real": {"id": candidates[0]["id"],
                       "physical_index": candidates[0]["physical_index"]}}
              if candidates else {"Dummy": {"physical_index": len(records)}})
    assert run["locate"] == {
        "accepted_positions": positions, "boundary_rejected_positions": boundaries,
        "candidates": candidates, "duplicate_count": duplicates,
        "truncated_count": max(0, len(candidates)-1), "target": target,
    }

    def selected(ids):
        if not ids:
            return None
        record = next(r for r in records if r["id"] == ids[0])
        return {"id": record["id"], "payload": record["payload"]}

    assert run["output"] == selected([c["id"] for c in candidates])
    assert run["fetch_count"] == 1
    assert run["ckks_metrics"]["openfhe_version"] == "1.5.1"
    windows, semantic_ids, exact_ids, decisions = [], [], [], []
    counts = dict.fromkeys(EFFECTS, 0)
    for record, span in zip(records, spans):
        for offset in range(max(0, len(record["searchable"])-len(pattern)+1)):
            window = record["searchable"][offset:offset+len(pattern)]
            differences = [a-b for a, b in zip(window, pattern)]
            if request["mode"] == "Fps":
                score = sum(d*d for d in differences)
            else:
                score = sum(d*w for d, w in zip(
                    differences, request["mode"]["Rfps"]["randomizer"]))
            same = window == pattern
            exact = abs(score) <= threshold
            position = span["start"] + offset
            approximate = abs(scores[position]) <= threshold
            windows.append({
                "id": record["id"], "local_offset": offset,
                "global_position": position, "exact_score": score,
                "semantic_match": same,
            })
            if same and record["id"] not in semantic_ids:
                semantic_ids.append(record["id"])
            if exact and record["id"] not in exact_ids:
                exact_ids.append(record["id"])
            effects = {
                "intrinsic_cancellation": not same and score == 0,
                "threshold_only": not same and score != 0 and exact,
                "ckks_false_positive": not exact and approximate,
                "ckks_false_negative": exact and not approximate,
            }
            for key, value in effects.items():
                counts[key] += value
            decisions.append({
                "id": record["id"], "global_position": position,
                "exact_score": score, "approximate_score": scores[position],
                "semantic_match": same, "exact_accepted": exact,
                "approximate_accepted": approximate, **effects,
            })
    semantic_record, exact_record = selected(semantic_ids), selected(exact_ids)
    assert run["oracle"] == {
        "windows": windows, "semantic_ids": semantic_ids, "exact_ids": exact_ids,
        "semantic_record": semantic_record, "exact_record": exact_record,
    }
    assert run["decisions"] == decisions
    exact_agreement = run["output"] == exact_record
    semantic_agreement = run["output"] == semantic_record
    assert run["exact_selection_agreement"] == exact_agreement
    assert run["semantic_selection_agreement"] == semantic_agreement
    observed_id = run["output"]["id"] if run["output"] is not None else None
    assert observed_id == fixture["expected_id"]
    assert exact_agreement and semantic_agreement != fixture["semantic_negative"]

    timings = run["timings"]
    assert timings["request_ns"] >= sum(
        timings[phase+"_ns"] for phase in ("match", "locate", "fetch"))
    phases = run["ckks_metrics"]["phase_timings"]
    derived = (phases["client_query_ns"] + phases["server_evaluation_ns"]
               + phases["client_decrypt_idft_ns"] + timings["locate_ns"]
               + sum(timings["fetch"].values()))
    assert timings["derived_online_sum_ns"] == derived
    return counts


def validate(report, sizes):
    assert report["schema"] == "p0-d-composition-v1"
    assert report["bounded_fixture_gate_passed"] and not report["errors"]
    assert report["bounds"]["public_k"] == 1
    assert report["software"]["openfhe_rs_commit"] == "3e6a46b1377e89e09414cbca7c0c0140c5338397"
    assert report["software"]["openfhe_commit"] == "1306d14f8c26bb6150d3e6ad54f28dfe1007689e"
    assert len(report["cases"]) == len(FIXTURES)
    cases = {c["fixture"]["name"]: c for c in report["cases"]}
    assert set(cases) == FIXTURES
    counts = dict.fromkeys(EFFECTS, 0)
    for case in cases.values():
        effects = check_case(case["fixture"], case["run"], case["catalog"])
        assert case["expected_observed"]
        for key, value in effects.items():
            counts[key] += value
    assert counts["intrinsic_cancellation"] > 0 and counts["threshold_only"] > 0

    assert len(report["performance"]) == len(PROBES)
    assert {p["fixture"]["name"] for p in report["performance"]} == set(PROBES.values())
    for performance in report["performance"]:
        fixture = cases[performance["fixture"]["name"]]["fixture"]
        assert performance["fixture"] == fixture
        assert performance["warmup_count"] == 1
        assert performance["warmup_excluded_from_statistics"]
        assert performance["all_expected"]
        assert len(performance["repetitions"]) == 5
        check_case(fixture, performance["warmup"])
        for run in performance["repetitions"]:
            check_case(fixture, run)
        for phase in ("match", "locate", "fetch", "request", "derived_online"):
            key = "derived_online_sum_ns" if phase == "derived_online" else phase+"_ns"
            values = [r["timings"][key] for r in performance["repetitions"]]
            summary = performance["statistics"][phase]
            assert summary["minimum_ns"] == min(values)
            assert summary["maximum_ns"] == max(values)
            assert summary["median_ns"] == statistics.median(values)
            assert abs(summary["mean_ns"]-statistics.mean(values)) < 1

    bgv = report["serialization"]["bgv"]
    assert bgv["roundtrip_exact"] and bgv["evaluation_key_bytes"] is None
    assert bgv["openfhe_version"] == "1.5.1" and not bgv["artifact_files_retained"]
    assert bgv["query_bytes"] > 0 and bgv["response_bytes"] > 0
    assert sizes["schema"] == "p0d-ckks-size-probe-v1"
    assert not sizes["artifacts_retained"]
    assert len(sizes["probes"]) == len(PROBES)
    assert {p["protocol"] for p in sizes["probes"]} == set(PROBES)
    communication = []
    for probe in sizes["probes"]:
        case = cases[PROBES[probe["protocol"]]]
        assert probe["fixture"] == case["fixture"]["name"]
        assert probe["text"] == case["catalog"]["text"]
        assert probe["pattern"] == case["fixture"]["request"]["pattern"]
        assert probe["request"] == case["fixture"]["request"]
        assert probe["openfhe_version"] == "1.5.1"
        assert probe["positions_agree_with_timing_run"]
        assert len(probe["scores"]) == len(case["run"]["scores"])
        assert all(math.isfinite(score) for score in probe["scores"])
        accepted = [i for i, score in enumerate(probe["scores"])
                    if abs(score) <= probe["request"]["threshold"]]
        assert probe["matching_positions"] == accepted
        assert accepted == case["run"]["locate"]["accepted_positions"]
        assert len(probe["query_ciphertext_bytes"]) == 2
        assert all(length > 0 for length in probe["query_ciphertext_bytes"])
        assert len(probe["setup_ciphertext_bytes"]) == (2 if probe["protocol"] == "fps" else 1)
        assert all(length > 0 for length in probe["setup_ciphertext_bytes"])
        for key in ("context_bytes", "public_key_bytes", "private_key_bytes",
                    "evaluation_key_bytes", "response_ciphertext_bytes"):
            assert probe[key] > 0
        communication.append({
            "protocol": probe["protocol"],
            "client_to_server_bytes": sum(probe["query_ciphertext_bytes"])+bgv["query_bytes"],
            "server_to_client_bytes": probe["response_ciphertext_bytes"]+bgv["response_bytes"],
            "excluded": "context, keys and query-independent setup; sizes are archive lengths from separate probes",
        })
    return {
        "validation": "passed", "case_count": len(cases),
        "decision_effect_counts": counts, "online_communication": communication,
    }


if __name__ == "__main__":
    result = validate(json.loads(Path(sys.argv[1]).read_text()),
                      json.loads(Path(sys.argv[2]).read_text()))
    print(json.dumps(result, indent=2))
