"""Frozen BGL K=1 inputs and an independent plaintext oracle."""
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import random
import sys


SCHEMA = "p0d-bgl-k1-study-v1"
SEED = "p0d-bgl-k1-20260911-v1"
SOURCE_SHA256 = "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496"
SOURCE_LINES = 2_000
SOURCE_COMMIT = "dd61d0952749ee7963bde24220d1be5ede023033"
SELECTED_RECORDS = 24
THRESHOLDS = (0.5, 1.0, 4.0)
WEIGHT_DRAWS = 16


def _source_rows(path):
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != SOURCE_SHA256:
        raise ValueError(f"unexpected source SHA-256: {digest}")
    lines = raw.splitlines()
    if len(lines) != SOURCE_LINES:
        raise ValueError(f"unexpected source line count: {len(lines)}")
    rows = []
    seed = SEED.encode()
    for line_number, line in enumerate(lines, 1):
        parts = line.split(maxsplit=9)
        if len(parts) != 10:
            continue
        message = parts[9]
        if not 16 <= len(message) <= 40 or b"\0" in message or not message.isascii():
            continue
        rank = hashlib.sha256(
            seed + b"\0" + str(line_number).encode() + b"\0" + message
        ).digest()
        rows.append(
            {
                "line_number": line_number,
                "source_label": parts[0].decode("ascii"),
                "message": message,
                "rank": rank,
            }
        )
    return rows


def _catalog(rows):
    chosen = sorted(rows, key=lambda row: row["rank"])[:SELECTED_RECORDS]
    chosen.sort(key=lambda row: row["line_number"])
    records = [
        {
            "id": row["line_number"],
            "source_line": row["line_number"],
            "source_label": row["source_label"],
            "searchable": list(row["message"]),
            "payload": list(row["message"]),
        }
        for row in chosen
    ]
    payload_bytes = sum(len(row["message"]) for row in chosen)
    return {
        "schema": SCHEMA,
        "seed": SEED,
        "source": {
            "dataset": "Loghub BGL_2k",
            "commit": SOURCE_COMMIT,
            "path": "BGL/BGL_2k.log",
            "sha256": SOURCE_SHA256,
            "logical_lines": SOURCE_LINES,
            "record_definition": "raw suffix after first nine whitespace boundaries",
        },
        "eligible_records": len(rows),
        "selection": {
            "message_bytes_min": 16,
            "message_bytes_max": 40,
            "ascii_only": True,
            "zero_byte_forbidden": True,
            "rank": "SHA256(seed || 0x00 || decimal line || 0x00 || message)",
            "take": SELECTED_RECORDS,
            "final_order": "numeric RecordId ascending",
            "he_outcome_used": False,
        },
        "selected_record_ids": [record["id"] for record in records],
        "payload_bytes": payload_bytes,
        "framed_search_bytes": payload_bytes + len(records) - 1,
        "records": records,
    }


def _frame(records):
    text = bytearray()
    spans = []
    for index, record in enumerate(records):
        searchable = bytes(record["searchable"])
        start = len(text)
        text.extend(searchable)
        spans.append((start, len(text), record["id"]))
        if index + 1 < len(records):
            text.append(0)
    return bytes(text), spans


def _score(window, pattern, mode, weights):
    if mode == "fps":
        return sum((left - right) ** 2 for left, right in zip(window, pattern))
    return sum(
        (left - right) * weight
        for left, right, weight in zip(window, pattern, weights)
    )


def evaluate(records, pattern, mode, weights, threshold):
    """Scan records directly; scan the framed text only for boundary evidence."""
    pattern = bytes(pattern)
    if not pattern:
        raise ValueError("empty pattern")
    if mode not in {"fps", "rfps"}:
        raise ValueError("unknown Match mode")
    if mode == "fps":
        if weights not in (None, []):
            raise ValueError("FPS cannot have RFPS weights")
        weights = []
    elif len(weights or []) != len(pattern) or any(not 1 <= x <= 32 for x in weights):
        raise ValueError("invalid RFPS weights")
    if threshold < 0 or threshold != threshold or threshold in (float("inf"), float("-inf")):
        raise ValueError("invalid threshold")

    framed, spans = _frame(records)
    valid = []
    literal_positions = []
    exact_positions = []
    cancellations = []
    threshold_only = []
    literal_ids = set()
    candidate_ids = set()
    global_start = 0
    for record in records:
        searchable = bytes(record["searchable"])
        for local_offset in range(max(0, len(searchable) - len(pattern) + 1)):
            window = searchable[local_offset : local_offset + len(pattern)]
            position = global_start + local_offset
            score = _score(window, pattern, mode, weights)
            literal = window == pattern
            accepted = abs(score) <= threshold
            if literal:
                literal_positions.append(position)
                literal_ids.add(record["id"])
            if accepted:
                exact_positions.append(position)
                candidate_ids.add(record["id"])
                if not literal and score == 0:
                    cancellations.append(position)
                elif not literal:
                    threshold_only.append(position)
            valid.append(
                {
                    "global_position": position,
                    "record_id": record["id"],
                    "local_offset": local_offset,
                    "score": score,
                    "literal": literal,
                    "accepted": accepted,
                }
            )
        global_start += len(searchable) + 1

    boundary_positions = []
    framed_literal = []
    framed_windows = max(0, len(framed) - len(pattern) + 1)
    valid_position_set = {row["global_position"] for row in valid}
    for position in range(framed_windows):
        window = framed[position : position + len(pattern)]
        if window == pattern:
            framed_literal.append(position)
        if position in valid_position_set:
            continue
        score = _score(window, pattern, mode, weights)
        if abs(score) <= threshold:
            boundary_positions.append(position)

    candidate_record_ids = sorted(candidate_ids)
    selected = candidate_record_ids[0] if candidate_record_ids else None
    payload = next(
        (record["payload"] for record in records if record["id"] == selected), None
    )
    literal_record_ids = sorted(literal_ids)
    semantic_selected = literal_record_ids[0] if literal_record_ids else None
    return {
        "count_units": {
            "valid_windows": "per valid within-record offset",
            "boundary_windows": "per framed offset not contained in one record",
            "candidate_records": "per distinct RecordId",
            "request": "one pattern, mode, weights, and threshold",
        },
        "framed_windows": framed_windows,
        "valid_windows": len(valid),
        "boundary_windows": framed_windows - len(valid),
        "literal_valid_positions": literal_positions,
        "literal_record_ids": literal_record_ids,
        "framed_literal_positions": framed_literal,
        "exact_accepted_valid_positions": exact_positions,
        "exact_accepted_boundary_positions": boundary_positions,
        "intrinsic_cancellation_positions": cancellations,
        "threshold_only_positions": threshold_only,
        "valid_candidate_record_ids": candidate_record_ids,
        "duplicate_candidate_offsets": len(exact_positions) - len(candidate_record_ids),
        "selected_record_id": selected,
        "selected_payload": payload,
        "semantic_selected_record_id": semantic_selected,
    }


def _literal_properties(records, pattern):
    result = evaluate(records, pattern, "fps", None, 0.5)
    return {
        "literal_occurrences": len(result["literal_valid_positions"]),
        "literal_record_ids": result["literal_record_ids"],
        "framed_occurrences": len(result["framed_literal_positions"]),
        "boundary_occurrences": len(result["exact_accepted_boundary_positions"]),
    }


def _occurrences(records, length):
    found = defaultdict(list)
    for record in records:
        searchable = bytes(record["searchable"])
        for offset in range(len(searchable) - length + 1):
            found[searchable[offset : offset + length]].append((record["id"], offset))
    return found


def _pattern(pattern_id, kind, evidence_class, value, records, source=None):
    return {
        "id": pattern_id,
        "kind": kind,
        "evidence_class": evidence_class,
        "length": len(value),
        "pattern": list(value),
        "ascii_display": value.decode("ascii", errors="backslashreplace"),
        "source": source,
        "selection_oracle": _literal_properties(records, value),
    }


def _patterns(records, all_rows, selected_ids):
    patterns = []
    for length in (8, 16, 32):
        found = _occurrences(records, length)
        unique = min(value for value, positions in found.items() if len(positions) == 1)
        multiple = min(
            value
            for value, positions in found.items()
            if len({record_id for record_id, _ in positions}) >= 2
        )
        patterns.append(_pattern(f"unique_{length:02}", "unique", "natural", unique, records))
        patterns.append(
            _pattern(
                f"multi_record_{length:02}", "multi_record", "natural", multiple, records
            )
        )
        chosen = None
        chosen_source = None
        for row in sorted(all_rows, key=lambda item: item["rank"]):
            if row["line_number"] in selected_ids or len(row["message"]) < length:
                continue
            for offset in range(len(row["message"]) - length + 1):
                candidate = row["message"][offset : offset + length]
                if candidate not in found:
                    chosen = candidate
                    chosen_source = {
                        "source_line": row["line_number"],
                        "local_offset": offset,
                    }
                    break
            if chosen is not None:
                break
        if chosen is None:
            raise ValueError(f"no excluded absent pattern for length {length}")
        patterns.append(
            _pattern(
                f"excluded_absent_{length:02}",
                "excluded_absent",
                "natural",
                chosen,
                records,
                chosen_source,
            )
        )

    repeats = _occurrences(records, 5)
    repeated = min(
        value
        for value, positions in repeats.items()
        if any(
            sum(candidate_id == record_id for candidate_id, _ in positions) >= 2
            for record_id, _ in positions
        )
    )
    patterns.append(
        _pattern("within_record_repeat_05", "within_record_repeat", "natural", repeated, records)
    )

    first = bytes(records[0]["searchable"])
    second = bytes(records[1]["searchable"])
    boundary = first[-4:] + b"\0" + second[:3]
    patterns.append(
        _pattern(
            "boundary_exact_08",
            "boundary_exact",
            "structural",
            boundary,
            records,
            {"left_record_id": records[0]["id"], "right_record_id": records[1]["id"]},
        )
    )

    base = bytes(next(item for item in patterns if item["id"] == "unique_08")["pattern"])
    valid_eight = _occurrences(records, 8)
    one_change = None
    for index, value in enumerate(base):
        for delta in (1, -1):
            changed = value + delta
            if not 0 <= changed <= 255:
                continue
            candidate = bytearray(base)
            candidate[index] = changed
            if bytes(candidate) not in valid_eight:
                one_change = bytes(candidate)
                one_source = {"base_pattern_id": "unique_08", "changes": [[index, delta]]}
                break
        if one_change is not None:
            break
    if one_change is None:
        raise ValueError("no one-change diagnostic")
    patterns.append(
        _pattern("one_change_08", "one_change", "controlled", one_change, records, one_source)
    )

    balanced = None
    for first_index in range(len(base)):
        if base[first_index] == 255:
            continue
        for second_index in range(first_index + 1, len(base)):
            if base[second_index] == 0:
                continue
            candidate = bytearray(base)
            candidate[first_index] += 1
            candidate[second_index] -= 1
            if bytes(candidate) not in valid_eight:
                balanced = bytes(candidate)
                balanced_source = {
                    "base_pattern_id": "unique_08",
                    "changes": [[first_index, 1], [second_index, -1]],
                }
                break
        if balanced is not None:
            break
    if balanced is None:
        raise ValueError("no balanced-change diagnostic")
    patterns.append(
        _pattern(
            "balanced_change_08",
            "balanced_change",
            "controlled",
            balanced,
            records,
            balanced_source,
        )
    )
    return {
        "schema": SCHEMA,
        "selection_used_he_outcomes": False,
        "patterns": patterns,
    }


def _weights(pattern_id, draw, length):
    label = f"{SEED}/weights/{pattern_id}/{draw}"
    source = random.Random(int.from_bytes(hashlib.sha256(label.encode()).digest(), "big"))
    return [source.randrange(3, 33) for _ in range(length)]


def _cases(records, patterns):
    cases = []

    def add(pattern, mode, policy, draw, weights, threshold):
        case_id = f'{pattern["id"]}/{mode}/{policy}/{draw if draw is not None else "none"}/{threshold:g}'
        cases.append(
            {
                "schema": SCHEMA,
                "id": case_id,
                "pattern_id": pattern["id"],
                "pattern_kind": pattern["kind"],
                "evidence_class": pattern["evidence_class"],
                "pattern": pattern["pattern"],
                "mode": mode,
                "weight_policy": policy,
                "weight_draw": draw,
                "weights": weights,
                "threshold": threshold,
                "oracle": evaluate(records, pattern["pattern"], mode, weights, threshold),
            }
        )

    for pattern in patterns:
        add(pattern, "fps", "none", None, [], 0.5)
    for pattern in patterns:
        for draw in range(WEIGHT_DRAWS):
            weights = _weights(pattern["id"], draw, pattern["length"])
            for threshold in THRESHOLDS:
                add(pattern, "rfps", "uniform_3_32", draw, weights, threshold)
    for pattern in patterns:
        if pattern["kind"] not in {"one_change", "balanced_change"}:
            continue
        weights = [1] * pattern["length"]
        for threshold in THRESHOLDS:
            add(pattern, "rfps", "ones_diagnostic", 0, weights, threshold)
    return cases


def build_study(source_path):
    rows = _source_rows(Path(source_path))
    catalog = _catalog(rows)
    selected_ids = set(catalog["selected_record_ids"])
    patterns = _patterns(catalog["records"], rows, selected_ids)
    cases = _cases(catalog["records"], patterns["patterns"])
    return {"catalog": catalog, "patterns": patterns, "cases": cases}


def write_study(source_path, output_directory):
    output = Path(output_directory)
    output.mkdir(parents=True, exist_ok=False)
    study = build_study(source_path)
    for name in ("catalog", "patterns"):
        with (output / f"{name}.json").open("x") as destination:
            json.dump(study[name], destination, indent=2)
            destination.write("\n")
    with (output / "cases.jsonl").open("x") as destination:
        for case in study["cases"]:
            destination.write(json.dumps(case, separators=(",", ":")) + "\n")
    with (output / "study_config.json").open("x") as destination:
        json.dump(
            {
                "schema": SCHEMA,
                "seed": SEED,
                "records": SELECTED_RECORDS,
                "patterns": len(study["patterns"]["patterns"]),
                "fps_attempts": 13,
                "uniform_rfps_attempts": 624,
                "ones_diagnostic_attempts": 6,
                "total_attempts": len(study["cases"]),
                "thresholds": THRESHOLDS,
                "uniform_weight_draws_per_pattern": WEIGHT_DRAWS,
                "rates_must_not_be_pooled_with_synthetic": True,
            },
            destination,
            indent=2,
        )
        destination.write("\n")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: generate.py BGL_2k.log NEW_OUTPUT_DIRECTORY")
    write_study(Path(sys.argv[1]), Path(sys.argv[2]))
