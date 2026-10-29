"""Independent, input-bound checks and empirical counts; not a security proof."""
from collections import defaultdict
import csv
import itertools
import json
import math
from pathlib import Path
import sys

from generate import THRESHOLDS, iter_plain_trials, native_trials

EFFECT_KEYS = ("intrinsic_cancellation", "threshold_only", "ckks_false_positive", "ckks_false_negative")
GROUP_KEYS = ("source", "policy", "family", "length", "threshold")
WINDOW_FIELDS = (
    "source", "trial_id", "policy", "family", "length", "threshold",
    "record_id", "global_position", "exact_score", "approximate_score",
    "semantic_match", "exact_accepted", "approximate_accepted",
    *EFFECT_KEYS,
)


def effects(exact, approximate, same, threshold):
    accepted = abs(exact) <= threshold
    observed = abs(approximate) <= threshold
    return {"intrinsic_cancellation": not same and exact == 0,
            "threshold_only": not same and exact != 0 and accepted,
            "ckks_false_positive": not accepted and observed,
            "ckks_false_negative": accepted and not observed}


def window_row(trial, source, threshold, exact, approximate, same, record_id, position):
    return {"source": source, "trial_id": trial["id"], "policy": trial["policy"],
            "family": trial["family"], "length": trial["length"], "threshold": threshold,
            "record_id": record_id, "global_position": position, "exact_score": exact,
            "approximate_score": approximate, "semantic_match": same,
            "exact_accepted": abs(exact) <= threshold,
            "approximate_accepted": abs(approximate) <= threshold,
            **effects(exact, approximate, same, threshold)}


def inspect_run(inp, threshold, run):
    """Check this study's equal-length record windows, without component oracles."""
    trial = inp["trial"]
    pattern, weights = trial["pattern"], trial["weights"]
    records = sorted(inp["records"], key=lambda r: r["id"])
    text, spans = [], []
    for index, record in enumerate(records):
        assert len(record["searchable"]) == len(pattern)
        if index:
            text.append(0)
        start = len(text)
        text.extend(record["searchable"])
        spans.append((start, len(text)))
    scores = run["scores"]
    assert len(scores) == len(text)-len(pattern)+1
    assert all(math.isfinite(score) for score in scores)
    accepted = [p for p, score in enumerate(scores) if abs(score) <= threshold]
    candidates, rejected = [], []
    for position in accepted:
        index = next((i for i, (a, b) in enumerate(spans)
                      if a <= position and position+len(pattern) <= b), None)
        if index is None:
            rejected.append(position)
        else:
            candidates.append({"id": records[index]["id"], "physical_index": index,
                               "global_position": position, "local_offset": position-spans[index][0],
                               "score": scores[position]})
    target = ({"Real": {"id": candidates[0]["id"], "physical_index": candidates[0]["physical_index"]}}
              if candidates else {"Dummy": {"physical_index": len(records)}})
    assert run["locate"] == {"accepted_positions": accepted, "boundary_rejected_positions": rejected,
                             "candidates": candidates, "duplicate_count": 0,
                             "truncated_count": max(0, len(candidates)-1), "target": target}

    def selected(ids):
        if not ids:
            return None
        record = next(r for r in records if r["id"] == ids[0])
        return {"id": record["id"], "payload": record["payload"]}

    assert run["output"] == selected([c["id"] for c in candidates])
    assert run["fetch_count"] == 1
    assert run["ckks_metrics"]["openfhe_version"] == "1.5.1"
    rows, oracle_windows, decisions, exact_ids, semantic_ids = [], [], [], [], []
    for record, (start, _) in zip(records, spans):
        # Independent direct loop: no generator score or P0-C reference helper.
        exact = 0
        for index in range(len(pattern)):
            exact += (record["searchable"][index]-pattern[index])*weights[index]
        same = record["searchable"] == pattern
        row = window_row(trial, "native", threshold, exact, scores[start], same, record["id"], start)
        rows.append(row)
        if same:
            semantic_ids.append(record["id"])
        if row["exact_accepted"]:
            exact_ids.append(record["id"])
        oracle_windows.append({"id": record["id"], "local_offset": 0, "global_position": start,
                               "exact_score": exact, "semantic_match": same})
        decisions.append({"id": record["id"], "global_position": start,
                          **{key: row[key] for key in ("exact_score", "approximate_score", "semantic_match",
                                                     "exact_accepted", "approximate_accepted", *EFFECT_KEYS)}})
    expected_exact, expected_semantic = selected(exact_ids), selected(semantic_ids)
    assert run["oracle"] == {"windows": oracle_windows, "exact_ids": exact_ids,
                             "semantic_ids": semantic_ids, "exact_record": expected_exact,
                             "semantic_record": expected_semantic}
    assert run["decisions"] == decisions
    assert run["exact_selection_agreement"] == (run["output"] == expected_exact)
    assert run["semantic_selection_agreement"] == (run["output"] == expected_semantic)
    timings = run["timings"]
    assert timings["request_ns"] >= sum(timings[k+"_ns"] for k in ("match", "locate", "fetch"))
    phase = run["ckks_metrics"]["phase_timings"]
    derived = sum(phase[k] for k in ("client_query_ns", "server_evaluation_ns", "client_decrypt_idft_ns"))
    assert timings["derived_online_sum_ns"] == derived+timings["locate_ns"]+sum(timings["fetch"].values())
    if run["output"] is not None:
        outcome = "correct_record" if run["output"] == expected_semantic else "false_record"
    else:
        outcome = "missed_true" if expected_semantic is not None else "correct_empty"
    return rows, outcome


def validate_coverage(inputs, results):
    expected = {(inp["trial"]["id"], t): inp for inp in inputs for t in inp["thresholds"]}
    assert len(expected) == sum(len(inp["thresholds"]) for inp in inputs)
    assert len(results) == len(expected), "missing or extra native attempt rows"
    seen = set()
    for row in results:
        assert row["schema"] == "p0d-rfps-native-result-v1"
        key = row["trial_id"], row["threshold"]
        assert key in expected and key not in seen, "unexpected/duplicate native attempt"
        seen.add(key)
        assert row["input"] == expected[key], "native result is not bound to generated input"
        if row["error"] is not None:
            assert isinstance(row["error"], str) and row["error"] and row["run"] is None
        else:
            assert isinstance(row["run"], dict)
    assert seen == set(expected)


def summarize(rows, group_keys=GROUP_KEYS):
    groups = {}
    for row in rows:
        key = tuple(row[k] for k in group_keys)
        if key not in groups:
            groups[key] = {**dict(zip(group_keys, key)), "windows": 0, "nonmatch_windows": 0,
                           "true_windows": 0, "false_matches": 0, "false_rejections": 0,
                           "exact_false_matches": 0, **dict.fromkeys(EFFECT_KEYS, 0)}
        group = groups[key]
        same = row["semantic_match"]
        group["windows"] += 1
        group["nonmatch_windows"] += not same
        group["true_windows"] += same
        group["false_matches"] += not same and row["approximate_accepted"]
        group["false_rejections"] += same and not row["approximate_accepted"]
        group["exact_false_matches"] += not same and row["exact_accepted"]
        for effect in EFFECT_KEYS:
            group[effect] += row[effect]
    for group in groups.values():
        group["false_match_percent"] = (100*group["false_matches"]/group["nonmatch_windows"]
                                         if group["nonmatch_windows"] else None)
        group["false_rejection_percent"] = (100*group["false_rejections"]/group["true_windows"]
                                             if group["true_windows"] else None)
    return sorted(groups.values(), key=lambda row: tuple(row[k] for k in group_keys))


def read_lines(path):
    with path.open() as source:
        return [json.loads(line) for line in source]


def write_csv(path, rows, fieldnames=None):
    fields = list(rows[0]) if rows else fieldnames
    assert fields, f"no CSV columns for {path}"
    with path.open("x", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def analyze(directory):
    plaintext = read_lines(directory / "plaintext_trials.jsonl")
    sentinel = object()
    for actual, expected in itertools.zip_longest(plaintext, iter_plain_trials(), fillvalue=sentinel):
        assert actual == expected, "plaintext input coverage, seed binding, or score mismatch"
    inputs = read_lines(directory / "native_inputs.jsonl")
    assert inputs == list(native_trials()), "native sample selection or input mismatch"
    results = read_lines(directory / "native_results.jsonl")
    validate_coverage(inputs, results)
    plain_rows = []
    for trial in plaintext:
        exact = sum((trial["text"][i]-trial["pattern"][i])*trial["weights"][i]
                    for i in range(trial["length"]))
        assert exact == trial["exact_score"] and trial["text"] != trial["pattern"]
        for threshold in THRESHOLDS:
            plain_rows.append(window_row(trial, "exact", threshold, exact, exact, False, 10, 0))
    native_rows, outcomes, boundaries, errors = [], [], 0, []
    for result in results:
        inp, threshold, run = result["input"], result["threshold"], result["run"]
        trial = inp["trial"]
        if result["error"] is not None:
            outcome = "execution_error"
            errors.append({"trial_id": trial["id"], "threshold": threshold, "error": result["error"]})
        else:
            rows, outcome = inspect_run(inp, threshold, run)
            native_rows.extend(rows)
            boundaries += len(run["locate"]["boundary_rejected_positions"])
        outcomes.append({"trial_id": trial["id"], "family": trial["family"], "policy": trial["policy"],
                         "length": trial["length"], "threshold": threshold, "layout": inp["layout"],
                         "outcome": outcome, "request_ns": run["timings"]["request_ns"] if run else None,
                         "error": result["error"]})
    rates = summarize(plain_rows+native_rows)
    causes = summarize(plain_rows+native_rows, ("source", "policy", "threshold"))
    write_csv(directory / "rates.csv", rates)
    write_csv(directory / "cause_counts.csv", causes)
    write_csv(directory / "native_windows.csv", native_rows, WINDOW_FIELDS)
    write_csv(directory / "outcomes.csv", outcomes)
    outcome_groups = defaultdict(lambda: defaultdict(int))
    for row in outcomes:
        outcome_groups[(row["policy"], row["threshold"], row["layout"])][row["outcome"]] += 1
    outcome_counts = [{"policy": key[0], "threshold": key[1], "layout": key[2],
                       "attempts": sum(counts.values()), **{k: counts[k] for k in (
                           "correct_record", "false_record", "missed_true", "correct_empty", "execution_error")}}
                      for key, counts in sorted(outcome_groups.items())]
    write_csv(directory / "outcome_counts.csv", outcome_counts)
    summary = {"schema": "p0d-rfps-rate-analysis-v1", "coverage_validated": True,
               "plaintext_trials": len(plaintext), "plaintext_threshold_decisions": len(plain_rows),
               "native_attempts": len(results), "native_completed": len(results)-len(errors),
               "native_valid_windows": len(native_rows), "execution_errors": errors,
               "boundary_positions_rejected": boundaries,
               "counts_by_source_policy_threshold": causes, "outcome_counts": outcome_counts,
               "limitations": ["Synthetic strata, not a deployment or global RFPS error rate",
                               "Native subset has 10 mismatch inputs per family/length/policy",
                               "Paired inputs and thresholds are correlated; no pooled confidence claim",
                               "Each threshold has fresh HE randomness; errors are retained",
                               "Exact and native denominators differ; effects can overlap",
                               "No algorithm, HE parameter, or production-threshold fix"]}
    with (directory / "analysis.json").open("x") as output:
        json.dump(summary, output, indent=2)
    print(json.dumps({k: v for k, v in summary.items() if k not in (
        "counts_by_source_policy_threshold", "outcome_counts")}, indent=2))


if __name__ == "__main__":
    analyze(Path(sys.argv[1]))
