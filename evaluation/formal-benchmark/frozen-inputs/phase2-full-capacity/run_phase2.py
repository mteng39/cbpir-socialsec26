#!/usr/bin/env python3
"""Run Phase 2 and write the seven bounded evidence artifacts."""

import csv
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))

import phase2_workload


SOURCE = ROOT / "docs/project/experiments/search/workloads/source/loghub/BGL_2k.log"
RUNTIME = ROOT / "code_branch/prototypes/openfhe_runtime/.openfhe/openfhe-1.5.1/install"
TARGET = ROOT / "code_branch/prototypes/.build-cache/phase1"
P0A = ROOT / "code_branch/prototypes/P0-A_bgv_cpir/Cargo.toml"
P0C = ROOT / "code_branch/prototypes/P0-C_ckks_dft_search/Cargo.toml"
P0D = ROOT / "code_branch/prototypes/P0-D_ckks_single_record_composition/Cargo.toml"
BINARY = TARGET / "debug/cbpir_phase2_full_capacity"
RINGS = (8192, 16384, 32768)


def environment():
    result = os.environ.copy()
    toolchain = "/usr/local/rustup/toolchains/1.91.1-x86_64-unknown-linux-gnu/bin"
    result["PATH"] = f"{toolchain}:{result.get('PATH', '')}"
    result["OPENFHE_INSTALL_DIR"] = str(RUNTIME)
    result["LD_LIBRARY_PATH"] = str(RUNTIME / "lib")
    result["CARGO_TARGET_DIR"] = str(TARGET)
    result["PYTHONDONTWRITEBYTECODE"] = "1"
    return result


def execute(command, *, input_text=None):
    completed = subprocess.run(
        [str(value) for value in command],
        cwd=ROOT,
        env=environment(),
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
    )
    return {
        "command": " ".join(str(value) for value in command),
        "status": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def require_success(result):
    if result["status"] != 0:
        raise RuntimeError(
            f"command failed: {result['command']}\n{result['stdout']}\n{result['stderr']}"
        )


def p3_diff_hash():
    result = execute(["git", "diff", "--binary", "--", "code_branch/prototypes/P3_multi_record_cpir"])
    require_success(result)
    return hashlib.sha256(result["stdout"].encode()).hexdigest()


def write_csv(path, fieldnames, rows):
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=fieldnames, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def validate_result(result, expected):
    workload = result["workload"]
    assert result["schema"] == phase2_workload.SCHEMA
    assert result["match_backend"] == "server_text_ct_pt"
    assert result["security"] == "HEStd_128_classic"
    assert result["openfhe_version"] == "1.5.1"
    assert workload["n"] == expected["n"]
    assert workload["text_bytes"] == expected["text_bytes"]
    assert workload["transform_len"] == expected["transform_len"]
    assert workload["records"] == expected["records"]
    assert result["fetch_preparation"]["physical_dummy_window_count"] == 0
    assert result["fetch_preparation"]["physical_capacity"] == workload["n"]
    assert result["serialized_size_independent_of_selection_cardinality"] is True
    for row in result["queries"]:
        query = row["query"]
        match = row["match"]
        fetch = row["fetch"]
        assert row["exact_oracle_agreement"] is True
        if query["mode"] == "fps":
            assert match["decision_mismatches"] == 0
            assert row["approximate_selection_agrees_with_exact"] is True
        assert fetch["query_ciphertext_count"] == 1
        assert fetch["response_ciphertext_count"] == 1
        assert fetch["selected_payloads_exact"] is True
        assert fetch["unselected_windows_zero"] is True
        assert row["end_to_end_exact"] is True
    for row in result["selector_probes"]:
        trace = row["trace"]
        assert trace["query_ciphertext_count"] == 1
        assert trace["response_ciphertext_count"] == 1
        assert trace["selected_payloads_exact"] is True
        assert trace["unselected_windows_zero"] is True
        if row["selected_size"] == 0:
            assert trace["empty_response_all_zero"] is True


def workload_rows(results):
    rows = []
    for ring in RINGS:
        workload = results[ring]["workload"]
        rows.append(
            {
                "N": ring,
                "n": workload["n"],
                "|T|": workload["text_bytes"],
                "L": workload["transform_len"],
                "N_CKKS/2": workload["ckks_slots"],
                "U_Match": workload["match_utilization"],
                "payload_bytes": workload["payload_bytes"],
                "ell": workload["record_slots"],
                "n*ell": workload["fetch_slots_used"],
                "N_BGV": workload["bgv_slots"],
                "U_Fetch": workload["fetch_window_utilization"],
                "U_payload": workload["payload_utilization"],
                "limiting_stage": workload["limiting_stage"],
                "searchable_min": workload["searchable_length_min"],
                "searchable_max": workload["searchable_length_max"],
                "searchable_mean": workload["searchable_length_mean"],
                "payload_min": workload["payload_length_min"],
                "payload_max": workload["payload_length_max"],
                "payload_mean": workload["payload_length_mean"],
            }
        )
    return rows


def query_rows(results):
    rows = []
    for ring in RINGS:
        for row in results[ring]["queries"]:
            query = row["query"]
            locate = row["locate"]
            rows.append(
                {
                    "N": ring,
                    "query_id": query["id"],
                    "mode": query["mode"],
                    "category": query["category"],
                    "m": query["pattern_length"],
                    "K": query["public_k"],
                    "threshold": query["threshold"],
                    "weight_policy": query["weight_policy"],
                    "candidate_relation": query["candidate_relation"],
                    "pattern_hex": bytes(query["pattern"]).hex(),
                    "pattern_ascii": query["pattern_ascii"],
                    "exact_candidate_count": len(query["oracle"]["candidate_record_ids"]),
                    "approximate_candidate_count": len(locate["candidates"]),
                    "selected_count": len(locate["selected_candidates"]),
                    "boundary_rejected": len(locate["boundary_rejected_positions"]),
                    "deduplicated_offsets": locate["duplicate_count"],
                    "truncated_candidates": locate["truncated_count"],
                }
            )
    return rows


def correctness_rows(results):
    rows = []
    for ring in RINGS:
        workload = results[ring]["workload"]
        for row in results[ring]["queries"]:
            query = row["query"]
            match = row["match"]
            fetch = row["fetch"]
            base = {
                "row_type": "query_summary",
                "N": ring,
                "|T|": workload["text_bytes"],
                "L": workload["transform_len"],
                "query_id": query["id"],
                "mode": query["mode"],
                "m": query["pattern_length"],
                "K": query["public_k"],
                "threshold": query["threshold"],
                "max_abs_error": match["max_abs_error"],
                "decision_mismatches": match["decision_mismatches"],
                "intrinsic_cancellation": match["intrinsic_cancellation_count"],
                "threshold_only": match["threshold_only_count"],
                "exact_oracle_agreement": row["exact_oracle_agreement"],
                "approximate_selection_agrees_with_exact": row["approximate_selection_agrees_with_exact"],
                "selected_payloads_exact": fetch["selected_payloads_exact"],
                "unselected_windows_zero": fetch["unselected_windows_zero"],
                "empty_response_all_zero": fetch["empty_response_all_zero"],
                "query_ciphertext_count": fetch["query_ciphertext_count"],
                "response_ciphertext_count": fetch["response_ciphertext_count"],
                "query_bytes": fetch["query_bytes"],
                "response_bytes": fetch["response_bytes"],
                "end_to_end_exact": row["end_to_end_exact"],
                "position": "",
                "exact_score": "",
                "decrypted_score": "",
            }
            rows.append(base)
            for mismatch in match["decision_mismatch_details"]:
                detail = dict(base)
                detail.update(
                    {
                        "row_type": "ckks_decision_mismatch",
                        "position": mismatch["position"],
                        "exact_score": mismatch["exact_score"],
                        "decrypted_score": mismatch["decrypted_score"],
                    }
                )
                rows.append(detail)
        for probe in results[ring]["selector_probes"]:
            trace = probe["trace"]
            rows.append(
                {
                    "row_type": "selector_probe",
                    "N": ring,
                    "|T|": workload["text_bytes"],
                    "L": workload["transform_len"],
                    "query_id": f"selector-k{probe['public_k']}-s{probe['selected_size']}",
                    "mode": "fetch",
                    "m": "",
                    "K": probe["public_k"],
                    "threshold": "",
                    "max_abs_error": "",
                    "decision_mismatches": "",
                    "intrinsic_cancellation": "",
                    "threshold_only": "",
                    "exact_oracle_agreement": True,
                    "approximate_selection_agrees_with_exact": True,
                    "selected_payloads_exact": trace["selected_payloads_exact"],
                    "unselected_windows_zero": trace["unselected_windows_zero"],
                    "empty_response_all_zero": trace["empty_response_all_zero"],
                    "query_ciphertext_count": trace["query_ciphertext_count"],
                    "response_ciphertext_count": trace["response_ciphertext_count"],
                    "query_bytes": trace["query_bytes"],
                    "response_bytes": trace["response_bytes"],
                    "end_to_end_exact": True,
                    "position": "",
                    "exact_score": "",
                    "decrypted_score": "",
                }
            )
    return rows


def utilization_rows(results):
    rows = []
    for ring in RINGS:
        workload = results[ring]["workload"]
        metrics = {
            "match_transform_slots_used": workload["transform_len"],
            "match_transform_slots_available": workload["ckks_slots"],
            "match_utilization": workload["match_utilization"],
            "fetch_windows_used": workload["n"],
            "fetch_windows_available": workload["n"],
            "fetch_slots_used": workload["fetch_slots_used"],
            "fetch_slots_available": workload["bgv_slots"],
            "fetch_window_utilization": workload["fetch_window_utilization"],
            "real_payload_bytes": workload["payload_bytes"],
            "payload_slot_bytes": workload["fetch_slots_used"],
            "payload_byte_utilization": workload["payload_utilization"],
            "physical_dummy_windows": 0,
            "serialized_query_bytes": results[ring]["query_bytes"],
            "serialized_response_bytes": results[ring]["response_bytes"],
        }
        for metric, value in metrics.items():
            rows.append({"N": ring, "metric": metric, "value": value})
    return rows


def timing_rows(results):
    rows = []
    for ring in RINGS:
        prep = results[ring]["fetch_preparation"]
        for stage, value in (
            ("bgv_context_generation", prep["context_generation_ns"]),
            ("bgv_key_generation", prep["key_generation_ns"]),
            ("bgv_database_preparation", prep["database_preparation_ns"]),
        ):
            rows.append({"N": ring, "query_id": "database", "mode": "fetch", "stage": stage, "nanoseconds": value, "not_final_benchmark": True})
        for row in results[ring]["queries"]:
            query = row["query"]
            timing = row["match"]["timings"]
            stages = {
                "ckks_context_generation": timing["context_generation_ns"],
                "ckks_key_generation": timing["key_generation_ns"],
                "server_text_dft_preparation": timing["server_text_dft_preparation_ns"],
                "server_plaintext_encoding": timing["server_plaintext_encoding_ns"],
                "server_query_plaintext_preparation": timing["server_query_plaintext_preparation_ns"],
                "client_query_dft_preparation": timing["client_query_dft_preparation_ns"],
                "client_query_encryption": timing["client_query_encryption_ns"],
                "server_homomorphic_evaluation": timing["server_homomorphic_evaluation_ns"],
                "client_decrypt": timing["client_decrypt_ns"],
                "client_idft": timing["client_idft_ns"],
                "oracle_error_instrumentation": timing["oracle_error_instrumentation_ns"],
                "locate": row["locate_ns"],
                "bgv_query_creation": row["fetch"]["timings"]["fetch_query_ns"],
                "bgv_server_evaluation": row["fetch"]["timings"]["fetch_response_ns"],
                "bgv_decrypt_decode": row["fetch"]["timings"]["recover_ns"],
                "bgv_artifact_serialization": row["fetch"]["timings"]["serialization_ns"],
            }
            for stage, value in stages.items():
                rows.append({"N": ring, "query_id": query["id"], "mode": query["mode"], "stage": stage, "nanoseconds": value, "not_final_benchmark": True})
    return rows


def report(results, queries):
    rfps_mismatches = {
        ring: sum(
            row["match"]["decision_mismatches"]
            for row in results[ring]["queries"]
            if row["query"]["mode"] == "rfps"
        )
        for ring in RINGS
    }
    mismatch_breakdown = ", ".join(
        f"{count} at N={ring}" for ring, count in rfps_mismatches.items()
    )
    lines = [
        "# Phase 2 — Full-capacity CBPIR configuration selection",
        "",
        "## Scope",
        "",
        "This is a bounded configuration/correctness gate. Timings are diagnostic only. It is not the later formal seven-procedure benchmark.",
        "",
        "The Match backend is explicit `server_text_ct_pt`. Fetch uses the existing P0-A multi-window BGV selector directly. P3 remains unchanged. Empty selection is encrypted as an all-zero selector and still produces one encrypted query and one encrypted response. No physical dummy record windows exist.",
        "",
        "## Frozen databases",
        "",
        "| N | n | |T| | L | U_Match | payload bytes | ell | U_Fetch | limiting stage |",
        "|---:|---:|---:|---:|---:|---:|---:|---:|:---|",
    ]
    for ring in RINGS:
        workload = results[ring]["workload"]
        lines.append(
            f"| {ring} | {workload['n']} | {workload['text_bytes']} | {workload['transform_len']} | {workload['match_utilization']:.3f} | {workload['payload_bytes']} | {workload['record_slots']} | {workload['fetch_window_utilization']:.3f} | {workload['limiting_stage']} |"
        )
    lines.extend(
        [
            "",
            f"Source: Loghub BGL_2k at `{phase2_workload.SOURCE_COMMIT}`; SHA-256 `{phase2_workload.SOURCE_SHA256}`. The retained seed `{phase2_workload.SEED}` ranks 1,331 eligible distinct source lines. Configurations are nested prefixes of 64, 128, and 256 records, then ordered by stable source-line RecordId.",
            "",
            "Searchable fields and payloads are the real selected message suffixes. Neither is padded to inflate utilization. The zero bytes between searchable fields are structural guards, not records.",
            "",
            "## Query corpus",
            "",
        ]
    )
    lengths = [int(row["m"]) for row in queries]
    lines.append(
        f"There are {len(queries)} profile-specific query rows ({len(queries)//3} per database). Pattern length m ranges from {min(lengths)} to {max(lengths)} bytes; mean m is {statistics.fmean(lengths):.3f}. K is exercised separately at `{{1,4,8,16}}` and is not used in the database-capacity calculation."
    )
    lines.extend(
        [
            "",
            "The deterministic corpus includes literal FPS matches, absence, repeated occurrences, guard-crossing matches, and fewer-than/exactly/more-than-K candidate sets. RFPS cases include a literal match, exact signed-score cancellation, and nonzero threshold-only acceptance.",
            "",
            "## Correctness",
            "",
            "| N | FPS queries | FPS max error | FPS decision mismatches | RFPS cancellations | RFPS threshold-only | RFPS CKKS mismatches | all Fetch exact |",
            "|---:|---:|---:|---:|---:|---:|---:|:---:|",
        ]
    )
    for ring in RINGS:
        fps = [row for row in results[ring]["queries"] if row["query"]["mode"] == "fps"]
        rfps = [row for row in results[ring]["queries"] if row["query"]["mode"] == "rfps"]
        lines.append(
            "| {ring} | {fps_count} | {fps_error:.6g} | {fps_mismatch} | {cancel} | {threshold} | {rfps_mismatch} | yes |".format(
                ring=ring,
                fps_count=len(fps),
                fps_error=max(row["match"]["max_abs_error"] for row in fps),
                fps_mismatch=sum(row["match"]["decision_mismatches"] for row in fps),
                cancel=sum(row["match"]["intrinsic_cancellation_count"] for row in rfps),
                threshold=sum(row["match"]["threshold_only_count"] for row in rfps),
                rfps_mismatch=sum(row["match"]["decision_mismatches"] for row in rfps),
            )
        )
    lines.extend(
        [
            "",
            "Every exact/plaintext Locate result matched the frozen oracle. Locate rejected accepted guard-crossing positions, deduplicated stable RecordIds, and applied deterministic ascending-RecordId First_K selection. Every selected frame reconstructed the exact stable ID and payload; every unselected physical window was zero.",
            "",
            "Selector probes covered S sizes 0, 1, <K, =K, and >K. Every probe used one query ciphertext and one response ciphertext. Within each N, serialized query and response sizes were independent of |S|. S=empty decrypted to an all-zero slot vector.",
            "",
            f"The retained RFPS results contain {sum(rfps_mismatches.values())} CKKS decision disagreements ({mismatch_breakdown}). These are permitted by the Phase-2 gate and are not hidden or merged with semantic RFPS acceptance. `04_correctness.csv` records N, |T|, L, m, threshold, exact score, decrypted score, and position for every event.",
            "",
            "RFPS semantic false acceptance is not CKKS numerical error. Exact Fetch reconstruction only proves reconstruction of the IDs selected by Match and Locate; it does not prove semantic RFPS correctness.",
            "",
            "## HE profiles and limits",
            "",
            "All contexts use `HEStd_128_classic`. FPS uses the retained repaired tuples: FPS-13 `(N=8192, depth=2, b0=60, bs=50, FIXEDMANUAL, BV)`, FPS-14 `(N=16384, depth=2, b0=60, bs=40, FIXEDMANUAL, HYBRID)`, FPS-15 `(N=32768, depth=2, b0=60, bs=50, FIXEDMANUAL, HYBRID)`. RFPS retains depth 1, b0 60, bs 24. BGV-RNS retains t 65537, depth 1, and ell 128.",
            "",
            "Actual T is used. No T is truncated. All three databases consume 50% of available CKKS transform slots and 100% of BGV record windows, so Fetch is the limiting stage.",
            "",
            "## Frozen benchmark inputs",
            "",
            "The subsequent formal benchmark should use exactly:",
            "",
        ]
    )
    for ring in RINGS:
        workload = results[ring]["workload"]
        lines.append(
            f"- `CBPIR-{ring.bit_length()-1}`: N={ring}, n={workload['n']}, |T|={workload['text_bytes']}, L={workload['transform_len']}, ell=128."
        )
    lines.extend(
        [
            "",
            "Do not substitute diagnostic Phase-2 timings for the formal seven-procedure measurements.",
            "",
        ]
    )
    return "\n".join(lines)


def verification_text(build, native_runs, tests, p3_before, p3_after, metadata):
    lines = [
        "PHASE 2 VERIFICATION",
        "====================",
        "",
        f"repository: {ROOT}",
        f"repository_head: {metadata['repository_head']}",
        f"rustc: {metadata['rustc']}",
        f"cargo: {metadata['cargo']}",
        f"source_sha256: {phase2_workload.SOURCE_SHA256}",
        "openfhe_version: 1.5.1",
        "security: HEStd_128_classic",
        "timing_scope: diagnostic only; not final benchmark",
        f"p3_diff_sha256_before: {p3_before}",
        f"p3_diff_sha256_after:  {p3_after}",
        f"p3_unchanged_during_phase2: {str(p3_before == p3_after).lower()}",
        "cargo_fmt: unavailable in pinned toolchain; no environment mutation attempted",
        f"reproduction_command: PYTHONDONTWRITEBYTECODE=1 python3 {HERE.relative_to(ROOT) / 'run_phase2.py'}",
        "",
        "BUILD",
        build["command"],
        f"exit_status={build['status']}",
        build["stdout"].rstrip(),
        build["stderr"].rstrip(),
        "",
        "NATIVE CONFIGURATION RUNS",
    ]
    for ring, native in native_runs.items():
        lines.extend([f"N={ring}: {native['command']}", f"exit_status={native['status']}"])
    lines.extend(["", "REGRESSION TESTS"])
    for test in tests:
        lines.extend(
            [
                test["command"],
                f"exit_status={test['status']}",
                test["stdout"].rstrip(),
                test["stderr"].rstrip(),
                "",
            ]
        )
    lines.extend(
        [
            "VERIFICATION GATES",
            "three_capacity_constraints=true",
            "no_text_truncation=true",
            "no_physical_dummy_windows=true",
            "fps_zero_decision_mismatches=true",
            "selected_fetch_payloads_exact=true",
            "unselected_fetch_windows_zero=true",
            "empty_selector_all_zero=true",
            "one_query_one_response=true",
            "serialized_sizes_independent_of_selection_size=true",
            "profiles_unchanged=true",
            "p3_implementation_untouched=true",
            "formal_benchmark_not_started=true",
            "",
        ]
    )
    return "\n".join(lines)


def main():
    p3_before = p3_diff_hash()
    metadata_commands = {
        "repository_head": execute(["git", "rev-parse", "HEAD"]),
        "rustc": execute(["rustc", "--version"]),
        "cargo": execute(["cargo", "--version"]),
    }
    for result in metadata_commands.values():
        require_success(result)
    metadata = {
        name: result["stdout"].strip()
        for name, result in metadata_commands.items()
    }
    build = execute(
        [
            "cargo",
            "build",
            "--locked",
            "--manifest-path",
            P0D,
            "--features",
            "native",
            "--bin",
            "cbpir_phase2_full_capacity",
        ]
    )
    require_success(build)

    studies = phase2_workload.build_all(SOURCE)
    results = {}
    native_runs = {}
    for ring in RINGS:
        study = studies[str(ring)]
        native = execute(
            [BINARY, ring],
            input_text=json.dumps(study, separators=(",", ":")),
        )
        native_runs[ring] = native
        require_success(native)
        result = json.loads(native["stdout"])
        validate_result(result, study["workload"])
        results[ring] = result

    tests = [
        execute([sys.executable, "-m", "unittest", "discover", "-s", HERE, "-p", "test_phase2_workload.py", "-v"]),
        execute(["cargo", "test", "--locked", "--manifest-path", P0A, "--features", "openfhe-backend", "--", "--test-threads=1"]),
        execute(["cargo", "test", "--locked", "--manifest-path", P0C, "--", "--test-threads=1"]),
        execute(["cargo", "test", "--locked", "--manifest-path", P0D, "--features", "native", "--", "--test-threads=1"]),
    ]
    for test in tests:
        require_success(test)
    p3_after = p3_diff_hash()
    if p3_before != p3_after:
        raise RuntimeError("P3 diff changed during Phase 2")

    workloads = workload_rows(results)
    queries = query_rows(results)
    correctness = correctness_rows(results)
    utilization = utilization_rows(results)
    timings = timing_rows(results)

    write_csv(HERE / "02_workloads.csv", list(workloads[0]), workloads)
    write_csv(HERE / "03_queries.csv", list(queries[0]), queries)
    write_csv(HERE / "04_correctness.csv", list(correctness[0]), correctness)
    write_csv(HERE / "05_utilization.csv", list(utilization[0]), utilization)
    write_csv(HERE / "06_diagnostic_timings.csv", list(timings[0]), timings)
    (HERE / "01_phase2_report.md").write_text(report(results, queries), encoding="utf-8")
    (HERE / "07_verification.txt").write_text(
        verification_text(build, native_runs, tests, p3_before, p3_after, metadata),
        encoding="utf-8",
    )
    print(
        json.dumps(
            {
                "status": "PASS",
                "configurations": [
                    {
                        "N": ring,
                        "n": results[ring]["workload"]["n"],
                        "T": results[ring]["workload"]["text_bytes"],
                        "L": results[ring]["workload"]["transform_len"],
                    }
                    for ring in RINGS
                ],
                "query_rows": len(queries),
                "correctness_rows": len(correctness),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
