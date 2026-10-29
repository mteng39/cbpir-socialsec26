"""Frozen synthetic RFPS study inputs. Simulation randomness is not HE randomness."""
import hashlib
import json
from pathlib import Path
import random
import sys

SEED = "p0d-rfps-rates-20260910-v1"
LENGTHS = (2, 4, 8, 16, 32, 64)
FAMILIES = ("full_bytes", "small_alphabet", "one_change", "balanced")
POLICIES = ("uniform_3_32", "ones_diagnostic")
THRESHOLDS = (0.5, 1.0, 4.0)
SAMPLES = 1000
NATIVE_INDICES = tuple(range(0, 1000, 100))


def rng(role, family, length, sample):
    label = f"{SEED}/{role}/{family}/{length}/{sample}"
    return random.Random(int.from_bytes(hashlib.sha256(label.encode()).digest(), "big"))


def exact_score(text, pattern, weights):
    assert len(text) == len(pattern) == len(weights)
    return sum((a-b)*w for a, b, w in zip(text, pattern, weights))


def make_trial(family, length, sample, policy):
    assert family in (*FAMILIES, "true_match") and length in LENGTHS and policy in POLICIES
    source = rng("input", family, length, sample)
    upper = 4 if family == "small_alphabet" else 255 if family in ("one_change", "balanced") else 256
    lower = 1 if family == "balanced" else 0
    pattern = [source.randrange(lower, upper) for _ in range(length)]
    text = pattern.copy()
    if family in ("full_bytes", "small_alphabet"):
        while text == pattern:
            text = [source.randrange(upper) for _ in range(length)]
    elif family == "one_change":
        text[source.randrange(length)] += 1
    elif family == "balanced":
        first, second = source.sample(range(length), 2)
        text[first] += 1
        text[second] -= 1
    weights_rng = rng("weights", family, length, sample)
    weights = [weights_rng.randrange(3, 33) for _ in range(length)] if policy == POLICIES[0] else [1]*length
    return {
        "id": f"{family}/{length}/{policy}/{sample}", "family": family,
        "length": length, "policy": policy, "sample": sample, "seed": SEED,
        "text": text, "pattern": pattern, "weights": weights,
        "exact_score": exact_score(text, pattern, weights),
    }


def iter_plain_trials():
    for family in FAMILIES:
        for length in LENGTHS:
            for policy in POLICIES:
                for sample in range(SAMPLES):
                    yield make_trial(family, length, sample, policy)


def record(record_id, searchable):
    return {"id": record_id, "searchable": searchable, "payload": [record_id, 0, 255, 0]}


def native_input(trial, planted):
    if trial["family"] == "true_match":
        records, layout = [record(20, trial["pattern"])], "true_only"
    else:
        records, layout = [record(10, trial["text"])], "no_match"
        if planted:
            records.append(record(20, trial["pattern"]))
            layout = "competing_true"
    return {"schema": "p0d-rfps-native-input-v1", "trial": trial,
            "records": records, "thresholds": list(THRESHOLDS), "layout": layout}


def native_trials():
    for family in FAMILIES:
        for length in LENGTHS:
            for policy in POLICIES:
                for ordinal, sample in enumerate(NATIVE_INDICES):
                    yield native_input(make_trial(family, length, sample, policy), ordinal % 2 == 1)
    for length in LENGTHS:
        for policy in POLICIES:
            for sample in range(10):
                yield native_input(make_trial("true_match", length, sample, policy), False)


def write_lines(path, rows):
    with path.open("x") as output:
        for row in rows:
            output.write(json.dumps(row, separators=(",", ":")) + "\n")


def main(directory):
    directory.mkdir(parents=True, exist_ok=True)
    write_lines(directory / "plaintext_trials.jsonl", iter_plain_trials())
    write_lines(directory / "native_inputs.jsonl", native_trials())
    with (directory / "study_config.json").open("x") as output:
        json.dump({"schema": "p0d-rfps-rate-study-v1", "seed": SEED,
                   "lengths": LENGTHS, "families": FAMILIES, "policies": POLICIES,
                   "thresholds": THRESHOLDS, "samples_per_cell": SAMPLES,
                   "native_indices": NATIVE_INDICES, "plaintext_trials": 48000,
                   "native_trials": 600, "native_attempts": 1800}, output, indent=2)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
