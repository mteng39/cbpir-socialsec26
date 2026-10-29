"""Deterministic final fixed-window CBPIR workloads and capacity audit."""

import importlib.util
from pathlib import Path
import statistics


HERE = Path(__file__).resolve().parent
PHASE2_MODULE = HERE.parent / "scalability/workload.py"
SCHEMA = "cbpir-phase2-full-capacity-final-v1"
ALLOWED_WIDTHS = (64, 128, 224, 256, 384, 512)
RINGS = (8192, 16384, 32768)
FRAME_HEADER_SLOTS = 14


def _load_phase2_module():
    spec = importlib.util.spec_from_file_location("frozen_phase2_workload", PHASE2_MODULE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PHASE2 = _load_phase2_module()


def _summary(values, prefix):
    return {
        f"{prefix}_min": min(values),
        f"{prefix}_max": max(values),
        f"{prefix}_mean": statistics.fmean(values),
        f"{prefix}_median": statistics.median(values),
    }


def pool_audit(source_path):
    """Build the deterministic pool and its frame-contract slot map.

    The Rust audit runner later verifies every value through `encode_frame()`;
    this function supplies the deterministic expected contract used to build
    candidate prefixes before any HE execution.
    """
    rows = PHASE2.source_rows(source_path)
    records = PHASE2._records(rows, len(rows))
    searchable_lengths = [len(record["searchable"]) for record in records]
    payload_lengths = [len(record["payload"]) for record in records]
    required_slots_by_id = {
        record["id"]: FRAME_HEADER_SLOTS + len(record["payload"])
        for record in records
    }
    serialized_slots = list(required_slots_by_id.values())
    maximum = max(serialized_slots)
    result = {
        "eligible_record_count": len(records),
        "required_slots_by_id": required_slots_by_id,
        "maximum_serialized_slots": maximum,
        "all_fit_64": maximum <= 64,
        "widths_fitting_all": [width for width in ALLOWED_WIDTHS if maximum <= width],
        "source_sha256": PHASE2.SOURCE_SHA256,
        "selection_seed": PHASE2.SEED,
    }
    result.update(_summary(searchable_lengths, "raw_searchable"))
    result.update(_summary(payload_lengths, "raw_payload"))
    result.update(_summary(serialized_slots, "serialized_slots"))
    return result


def _prefix(rows, count, required_slots_by_id):
    records = PHASE2._records(rows, count)
    text, spans = PHASE2._frame(records)
    transform_len = PHASE2._next_power_of_two(len(text) + 1)
    maximum = max(required_slots_by_id[record["id"]] for record in records)
    return records, text, spans, transform_len, maximum


def build_window_matrix(source_path, required_slots_by_id):
    rows = PHASE2.source_rows(source_path)
    matrix = []
    for ring_dimension in RINGS:
        for width in ALLOWED_WIDTHS:
            fetch_maximum = ring_dimension // width
            tested_maximum = min(fetch_maximum, len(rows))
            feasible = None
            for count in range(1, tested_maximum + 1):
                records, text, _spans, transform_len, maximum = _prefix(
                    rows, count, required_slots_by_id
                )
                if maximum <= width and transform_len <= ring_dimension // 2:
                    feasible = (count, records, text, transform_len, maximum)
            if feasible is None:
                matrix.append(
                    {
                        "N": ring_dimension,
                        "w": width,
                        "n_fetch_max": fetch_maximum,
                        "n": 0,
                        "max_serialized_slots": max(required_slots_by_id.values()),
                        "text_bytes": 0,
                        "L": 0,
                        "N_CKKS/2": ring_dimension // 2,
                        "U_Match": 0.0,
                        "U_Fetch": 0.0,
                        "limiting_condition": "record_width",
                        "status": "INFEASIBLE",
                    }
                )
                continue

            count, _records, text, transform_len, maximum = feasible
            next_fetch_blocked = (count + 1) * width > ring_dimension
            next_match_blocked = False
            if count < len(rows):
                _r, _t, _s, next_transform, _m = _prefix(
                    rows, count + 1, required_slots_by_id
                )
                next_match_blocked = next_transform > ring_dimension // 2
            conditions = []
            if next_match_blocked:
                conditions.append("Match")
            if next_fetch_blocked:
                conditions.append("Fetch")
            if count == len(rows):
                conditions.append("eligible_pool")
            matrix.append(
                {
                    "N": ring_dimension,
                    "w": width,
                    "n_fetch_max": fetch_maximum,
                    "n": count,
                    "max_serialized_slots": maximum,
                    "text_bytes": len(text),
                    "L": transform_len,
                    "N_CKKS/2": ring_dimension // 2,
                    "U_Match": transform_len / (ring_dimension // 2),
                    "U_Fetch": count * width / ring_dimension,
                    "limiting_condition": "+".join(conditions) or "tested_prefix",
                    "status": "PASS",
                }
            )
    return matrix


def select_final_configurations(source_path, required_slots_by_id):
    rows = PHASE2.source_rows(source_path)
    selected = {}
    for ring_dimension in RINGS:
        best = None
        for count in range(1, len(rows) + 1):
            records, text, spans, transform_len, maximum = _prefix(
                rows, count, required_slots_by_id
            )
            fitting_widths = [width for width in ALLOWED_WIDTHS if maximum <= width]
            if not fitting_widths:
                continue
            width = min(fitting_widths)
            if count * width <= ring_dimension and transform_len <= ring_dimension // 2:
                best = {
                    "records": records,
                    "text": text,
                    "spans": spans,
                    "n": count,
                    "record_slots": width,
                    "transform_len": transform_len,
                    "max_serialized_slots": maximum,
                }
        if best is None:
            raise ValueError(f"no legal fixed-window configuration for N={ring_dimension}")
        if best["n"] == len(rows):
            best["limiting_stage"] = "eligible_pool"
        else:
            _next_records, _next_text, _next_spans, next_transform, next_maximum = _prefix(
                rows, best["n"] + 1, required_slots_by_id
            )
            next_widths = [width for width in ALLOWED_WIDTHS if next_maximum <= width]
            blockers = []
            if next_transform > ring_dimension // 2:
                blockers.append("Match")
            if not next_widths or (best["n"] + 1) * min(next_widths) > ring_dimension:
                blockers.append("Fetch")
            best["limiting_stage"] = "+".join(blockers) or "none"
        selected[ring_dimension] = best
    return selected


def _workload(rows, ring_dimension, selected, required_slots_by_id):
    records = selected["records"]
    text = selected["text"]
    width = selected["record_slots"]
    searchable_lengths = [len(record["searchable"]) for record in records]
    payload_lengths = [len(record["payload"]) for record in records]
    serialized_lengths = [required_slots_by_id[record["id"]] for record in records]
    payload_bytes = sum(payload_lengths)
    fetch_slots = len(records) * width
    return {
        "schema": SCHEMA,
        "ring_dimension": ring_dimension,
        "n": len(records),
        "text_bytes": len(text),
        "transform_len": selected["transform_len"],
        "ckks_slots": ring_dimension // 2,
        "match_utilization": selected["transform_len"] / (ring_dimension // 2),
        "payload_bytes": payload_bytes,
        "record_slots": width,
        "fetch_slots_used": fetch_slots,
        "bgv_slots": ring_dimension,
        "fetch_window_utilization": fetch_slots / ring_dimension,
        "payload_utilization": payload_bytes / fetch_slots,
        "max_serialized_slots": max(serialized_lengths),
        "serialized_slots_min": min(serialized_lengths),
        "serialized_slots_max": max(serialized_lengths),
        "serialized_slots_mean": statistics.fmean(serialized_lengths),
        "serialized_slots_median": statistics.median(serialized_lengths),
        "limiting_stage": selected["limiting_stage"],
        "searchable_length_min": min(searchable_lengths),
        "searchable_length_max": max(searchable_lengths),
        "searchable_length_mean": statistics.fmean(searchable_lengths),
        "searchable_length_median": statistics.median(searchable_lengths),
        "payload_length_min": min(payload_lengths),
        "payload_length_max": max(payload_lengths),
        "payload_length_mean": statistics.fmean(payload_lengths),
        "payload_length_median": statistics.median(payload_lengths),
        "selection_seed": PHASE2.SEED,
        "source_sha256": PHASE2.SOURCE_SHA256,
        "source_commit": PHASE2.SOURCE_COMMIT,
        "eligible_records": len(rows),
        "no_physical_dummy_windows": True,
        "records": records,
        "spans": selected["spans"],
        "required_slots_by_record": serialized_lengths,
    }


def build_all(source_path, required_slots_by_id):
    rows = PHASE2.source_rows(source_path)
    selected = select_final_configurations(source_path, required_slots_by_id)
    result = {}
    for ring_dimension in RINGS:
        workload = _workload(rows, ring_dimension, selected[ring_dimension], required_slots_by_id)
        queries = PHASE2._queries(workload["records"], rows)
        for query in queries:
            query["schema"] = SCHEMA
        result[str(ring_dimension)] = {"workload": workload, "queries": queries}
    return result
