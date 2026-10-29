"""Read-only adapter from the frozen Phase-2 configuration to formal inputs."""

import importlib.util
from pathlib import Path


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
FROZEN_MODULE = HERE / "final_workload.py"
SOURCE = ROOT / "data/source/loghub/BGL_2k.log"
SCHEMA = "cbpir-formal-seven-procedure-v1"
EXPECTED = {
    8192: {
        "profile": "CBPIR-13",
        "n": 128,
        "ell": 64,
        "text_bytes": 3201,
        "transform_len": 4096,
        "query_id": "fps-k16-exact-16",
        "pattern": b"f",
        "candidates": 16,
    },
    16384: {
        "profile": "CBPIR-14",
        "n": 256,
        "ell": 64,
        "text_bytes": 6544,
        "transform_len": 8192,
        "query_id": "fps-k16-exact-16",
        "pattern": b",",
        "candidates": 16,
    },
    32768: {
        "profile": "CBPIR-15",
        "n": 512,
        "ell": 64,
        "text_bytes": 13358,
        "transform_len": 16384,
        "query_id": "fps-k16-more-17",
        "pattern": b":",
        "candidates": 17,
    },
}


def _load_frozen_module():
    spec = importlib.util.spec_from_file_location("formal_frozen_workload", FROZEN_MODULE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


FROZEN = _load_frozen_module()


def build_inputs(warmups=3, measured_runs=20, modes=None, public_ks=None):
    """Build exact formal inputs without modifying frozen evidence."""
    modes = ["fps", "rfps"] if modes is None else list(modes)
    public_ks = [1, 4, 8, 16] if public_ks is None else list(public_ks)
    audit = FROZEN.pool_audit(SOURCE)
    studies = FROZEN.build_all(SOURCE, audit["required_slots_by_id"])
    result = {}
    for ring, expected in EXPECTED.items():
        frozen = studies[str(ring)]
        workload = frozen["workload"]
        actual_tuple = (
            workload["n"],
            workload["record_slots"],
            workload["text_bytes"],
            workload["transform_len"],
        )
        expected_tuple = (
            expected["n"],
            expected["ell"],
            expected["text_bytes"],
            expected["transform_len"],
        )
        if actual_tuple != expected_tuple:
            raise ValueError(f"frozen workload drift for N={ring}: {actual_tuple}")
        source_query = next(
            query for query in frozen["queries"] if query["id"] == expected["query_id"]
        )
        pattern = bytes(source_query["pattern"])
        if pattern != expected["pattern"]:
            raise ValueError(f"timing pattern drift for N={ring}: {pattern!r}")
        fps_candidates = len(source_query["oracle"]["candidate_record_ids"])
        rfps_oracle = FROZEN.PHASE2._oracle(
            workload["records"], pattern, "rfps", [3], 0.5, 16
        )
        rfps_candidates = len(rfps_oracle["candidate_record_ids"])
        if fps_candidates != expected["candidates"] or rfps_candidates != expected["candidates"]:
            raise ValueError(
                f"timing candidate drift for N={ring}: FPS={fps_candidates}, RFPS={rfps_candidates}"
            )
        probes = [
            {
                "id": query["id"],
                "category": query["category"],
                "pattern": query["pattern"],
                "weights": query["weights"],
                "threshold": query["threshold"],
                "public_k": query["public_k"],
            }
            for query in frozen["queries"]
            if query["mode"] == "rfps"
        ]
        result[ring] = {
            "schema": SCHEMA,
            "profile": expected["profile"],
            "workload": workload,
            "timing_fixture": {
                "source_query_id": expected["query_id"],
                "pattern": list(pattern),
                "pattern_ascii": pattern.decode("ascii"),
                "threshold": 0.5,
                "expected_candidate_count": expected["candidates"],
            },
            "rfps_probes": probes,
            "modes": modes,
            "public_ks": public_ks,
            "warmups": warmups,
            "measured_runs": measured_runs,
        }
    return result
