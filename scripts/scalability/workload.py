"""Deterministic Phase-2 BGL workloads and plaintext query oracles."""

from collections import defaultdict
import hashlib
from pathlib import Path
import random
import statistics


SCHEMA = "cbpir-phase2-full-capacity-v1"
SEED = "p0d-bgl-k1-20260911-v1"
SOURCE_SHA256 = "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496"
SOURCE_COMMIT = "dd61d0952749ee7963bde24220d1be5ede023033"
SOURCE_LINES = 2_000
RECORD_SLOTS = 128
PROFILE_RECORDS = {8192: 64, 16384: 128, 32768: 256}
PUBLIC_K = (1, 4, 8, 16)


def source_rows(path):
    raw = Path(path).read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != SOURCE_SHA256:
        raise ValueError(f"unexpected BGL SHA-256: {digest}")
    lines = raw.splitlines()
    if len(lines) != SOURCE_LINES:
        raise ValueError(f"unexpected BGL line count: {len(lines)}")
    seed = SEED.encode("ascii")
    rows = []
    for line_number, line in enumerate(lines, 1):
        parts = line.split(maxsplit=9)
        if len(parts) != 10:
            continue
        message = parts[9]
        if not 16 <= len(message) <= 40 or b"\0" in message or not message.isascii():
            continue
        rank = hashlib.sha256(
            seed + b"\0" + str(line_number).encode("ascii") + b"\0" + message
        ).hexdigest()
        rows.append(
            {
                "line_number": line_number,
                "source_label": parts[0].decode("ascii"),
                "message": message,
                "rank": rank,
            }
        )
    return rows


def _next_power_of_two(value):
    if value <= 0:
        raise ValueError("positive power-of-two input required")
    return 1 << (value - 1).bit_length()


def _records(rows, count):
    chosen = sorted(rows, key=lambda row: row["rank"])[:count]
    chosen.sort(key=lambda row: row["line_number"])
    return [
        {
            "id": row["line_number"],
            "source_line": row["line_number"],
            "source_label": row["source_label"],
            "searchable": list(row["message"]),
            "payload": list(row["message"]),
        }
        for row in chosen
    ]


def _frame(records):
    text = bytearray()
    spans = []
    for index, record in enumerate(records):
        if index:
            text.append(0)
        start = len(text)
        text.extend(record["searchable"])
        spans.append(
            {
                "id": record["id"],
                "physical_index": index,
                "start": start,
                "end": len(text),
            }
        )
    return bytes(text), spans


def _score(window, pattern, mode, weights):
    if mode == "fps":
        return sum((left - right) ** 2 for left, right in zip(window, pattern))
    return sum(
        (left - right) * weight
        for left, right, weight in zip(window, pattern, weights)
    )


def _oracle(records, pattern, mode, weights, threshold, public_k):
    text, spans = _frame(records)
    pattern = bytes(pattern)
    scores = [
        _score(text[position : position + len(pattern)], pattern, mode, weights)
        for position in range(len(text) - len(pattern) + 1)
    ]
    accepted = [position for position, score in enumerate(scores) if abs(score) <= threshold]
    literal_positions = []
    intrinsic = []
    threshold_only = []
    candidates = []
    boundary = []
    duplicate_count = 0
    for position in accepted:
        end = position + len(pattern)
        span = next(
            (span for span in spans if span["start"] <= position and end <= span["end"]),
            None,
        )
        window_matches = text[position:end] == pattern
        if window_matches:
            literal_positions.append(position)
        elif mode == "rfps" and scores[position] == 0:
            intrinsic.append(position)
        elif mode == "rfps":
            threshold_only.append(position)
        if span is None:
            boundary.append(position)
            continue
        if candidates and candidates[-1]["id"] == span["id"]:
            duplicate_count += 1
            continue
        candidates.append(
            {
                "id": span["id"],
                "physical_index": span["physical_index"],
                "global_position": position,
                "local_offset": position - span["start"],
                "exact_score": scores[position],
            }
        )
    candidates.sort(key=lambda candidate: (candidate["id"], candidate["global_position"]))
    selected = candidates[:public_k]
    return {
        "score_count": len(scores),
        "exact_scores": scores,
        "accepted_positions": accepted,
        "literal_positions": literal_positions,
        "boundary_rejected_positions": boundary,
        "candidate_record_ids": [candidate["id"] for candidate in candidates],
        "candidate_physical_indices": [candidate["physical_index"] for candidate in candidates],
        "selected_record_ids": [candidate["id"] for candidate in selected],
        "selected_physical_indices": [candidate["physical_index"] for candidate in selected],
        "duplicate_candidate_offsets": duplicate_count,
        "truncated_count": max(0, len(candidates) - public_k),
        "intrinsic_cancellation_positions": intrinsic,
        "intrinsic_cancellation_count": len(intrinsic),
        "threshold_only_positions": threshold_only,
        "threshold_only_count": len(threshold_only),
    }


def _occurrences(records, max_length=16):
    by_length = {}
    for length in range(1, max_length + 1):
        found = defaultdict(list)
        for record in records:
            value = bytes(record["searchable"])
            for offset in range(len(value) - length + 1):
                found[value[offset : offset + length]].append((record["id"], offset))
        by_length[length] = found
    return by_length


def _patterns_by_record_count(occurrences):
    result = defaultdict(list)
    for length, patterns in occurrences.items():
        for pattern, positions in patterns.items():
            count = len({record_id for record_id, _ in positions})
            result[count].append((length, pattern))
    for values in result.values():
        values.sort(key=lambda item: (item[0], item[1]))
    return result


def _absent_pattern(records, all_rows, length=16):
    present = set(_occurrences(records, length)[length])
    selected = {record["id"] for record in records}
    for row in sorted(all_rows, key=lambda item: item["rank"]):
        if row["line_number"] in selected:
            continue
        message = row["message"]
        for offset in range(len(message) - length + 1):
            candidate = message[offset : offset + length]
            if candidate not in present:
                return candidate
    raise ValueError("no deterministic absent BGL pattern")


def _weights(label, length):
    seed = int.from_bytes(hashlib.sha256(f"{SEED}/{label}".encode()).digest(), "big")
    source = random.Random(seed)
    return [source.randrange(3, 33) for _ in range(length)]


def _query(query_id, category, pattern, mode, weights, threshold, public_k, relation, records):
    oracle = _oracle(records, pattern, mode, weights, threshold, public_k)
    return {
        "schema": SCHEMA,
        "id": query_id,
        "category": category,
        "mode": mode,
        "pattern": list(pattern),
        "pattern_ascii": bytes(pattern).decode("ascii", errors="backslashreplace"),
        "pattern_length": len(pattern),
        "weights": list(weights),
        "weight_policy": "none" if mode == "fps" else ("ones_diagnostic" if set(weights) == {1} else "uniform_3_32"),
        "threshold": threshold,
        "public_k": public_k,
        "candidate_relation": relation,
        "oracle": oracle,
    }


def _controlled_rfps(records, base, category):
    present = set(_occurrences(records, len(base))[len(base)])
    if category == "rfps_threshold_only":
        for index, value in enumerate(base):
            for delta in (1, -1):
                changed = value + delta
                if not 0 <= changed <= 255:
                    continue
                pattern = bytearray(base)
                pattern[index] = changed
                if bytes(pattern) not in present:
                    return bytes(pattern)
    else:
        for first in range(len(base)):
            if base[first] == 255:
                continue
            for second in range(first + 1, len(base)):
                if base[second] == 0:
                    continue
                pattern = bytearray(base)
                pattern[first] += 1
                pattern[second] -= 1
                if bytes(pattern) not in present:
                    return bytes(pattern)
    raise ValueError(f"no controlled RFPS pattern for {category}")


def _queries(records, all_rows):
    occurrences = _occurrences(records)
    by_count = _patterns_by_record_count(occurrences)
    absent = _absent_pattern(records, all_rows)
    queries = []

    for public_k in PUBLIC_K:
        available = sorted(by_count)
        fewer_count = max((count for count in available if count < public_k), default=0)
        more_count = min(count for count in available if count > public_k)
        for relation, count in (("fewer", fewer_count), ("exact", public_k), ("more", more_count)):
            if count == 0:
                pattern = absent
                category = "fps_absent"
            else:
                pattern = by_count[count][0][1]
                category = "fps_multiplicity"
            queries.append(
                _query(
                    f"fps-k{public_k}-{relation}-{count}",
                    category,
                    pattern,
                    "fps",
                    [],
                    0.5,
                    public_k,
                    relation,
                    records,
                )
            )

    repeated = min(
        pattern
        for pattern, positions in occurrences[5].items()
        if any(
            sum(candidate_id == record_id for candidate_id, _ in positions) >= 2
            for record_id, _ in positions
        )
    )
    queries.append(
        _query("fps-within-record-repeat", "fps_within_record_repeat", repeated, "fps", [], 0.5, 4, "auxiliary", records)
    )
    first = bytes(records[0]["searchable"])
    second = bytes(records[1]["searchable"])
    boundary = first[-4:] + b"\0" + second[:3]
    queries.append(
        _query("fps-boundary", "fps_boundary", boundary, "fps", [], 0.5, 4, "auxiliary", records)
    )

    unique_eight = min(
        pattern
        for pattern, positions in occurrences[8].items()
        if len({record_id for record_id, _ in positions}) == 1
    )
    queries.append(
        _query("rfps-literal", "rfps_literal", unique_eight, "rfps", _weights("rfps-literal", 8), 0.5, 4, "diagnostic", records)
    )
    cancellation = _controlled_rfps(records, unique_eight, "rfps_cancellation")
    queries.append(
        _query("rfps-cancellation", "rfps_cancellation", cancellation, "rfps", [1] * 8, 0.5, 4, "diagnostic", records)
    )
    threshold_only = _controlled_rfps(records, unique_eight, "rfps_threshold_only")
    queries.append(
        _query("rfps-threshold-only", "rfps_threshold_only", threshold_only, "rfps", [1] * 8, 1.0, 4, "diagnostic", records)
    )
    return queries


def _workload(rows, ring_dimension):
    count = PROFILE_RECORDS[ring_dimension]
    records = _records(rows, count)
    text, spans = _frame(records)
    transform_len = _next_power_of_two(len(text) + 1)
    payload_lengths = [len(record["payload"]) for record in records]
    searchable_lengths = [len(record["searchable"]) for record in records]
    payload_bytes = sum(payload_lengths)
    return {
        "schema": SCHEMA,
        "ring_dimension": ring_dimension,
        "n": count,
        "text_bytes": len(text),
        "transform_len": transform_len,
        "ckks_slots": ring_dimension // 2,
        "match_utilization": transform_len / (ring_dimension // 2),
        "payload_bytes": payload_bytes,
        "record_slots": RECORD_SLOTS,
        "fetch_slots_used": count * RECORD_SLOTS,
        "bgv_slots": ring_dimension,
        "fetch_window_utilization": count * RECORD_SLOTS / ring_dimension,
        "payload_utilization": payload_bytes / (count * RECORD_SLOTS),
        "limiting_stage": "Fetch" if count * RECORD_SLOTS == ring_dimension else "Match",
        "searchable_length_min": min(searchable_lengths),
        "searchable_length_max": max(searchable_lengths),
        "searchable_length_mean": statistics.fmean(searchable_lengths),
        "payload_length_min": min(payload_lengths),
        "payload_length_max": max(payload_lengths),
        "payload_length_mean": statistics.fmean(payload_lengths),
        "selection_seed": SEED,
        "source_sha256": SOURCE_SHA256,
        "source_commit": SOURCE_COMMIT,
        "eligible_records": len(rows),
        "no_physical_dummy_windows": True,
        "records": records,
        "spans": spans,
    }


def build_all(source_path):
    rows = source_rows(source_path)
    result = {}
    for ring_dimension in PROFILE_RECORDS:
        workload = _workload(rows, ring_dimension)
        result[str(ring_dimension)] = {
            "workload": workload,
            "queries": _queries(workload["records"], rows),
        }
    return result
