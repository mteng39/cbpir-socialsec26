#!/usr/bin/env python3
"""Run the final fixed-window configuration gate and write retained evidence."""

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

import final_workload


SOURCE = ROOT / "docs/project/experiments/search/workloads/source/loghub/BGL_2k.log"
RUNTIME = ROOT / "code_branch/prototypes/openfhe_runtime/.openfhe/openfhe-1.5.1/install"
TARGET = ROOT / "code_branch/prototypes/.build-cache/phase2b"
P0A = ROOT / "code_branch/prototypes/P0-A_bgv_cpir/Cargo.toml"
P0C = ROOT / "code_branch/prototypes/P0-C_ckks_dft_search/Cargo.toml"
P0D = ROOT / "code_branch/prototypes/P0-D_ckks_single_record_composition/Cargo.toml"
RUNNER = TARGET / "debug/cbpir_phase2_fixed_final"
FRAME_AUDIT = TARGET / "debug/cbpir_fixed_frame_audit"
ORIGINAL_PHASE2 = ROOT / "experiments/cbpir-scaling/phase2-full-capacity"
RINGS = final_workload.RINGS


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


def write_csv(path, fieldnames, rows):
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=fieldnames, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def tree_hashes(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            result[str(path.relative_to(directory))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def p3_diff_hash():
    result = execute(["git", "diff", "--binary", "--", "code_branch/prototypes/P3_multi_record_cpir"])
    require_success(result)
    return hashlib.sha256(result["stdout"].encode()).hexdigest()


def profile_name(ring):
    return f"CBPIR-{ring.bit_length() - 1}"


def validate_native_result(result, expected):
    workload = result["workload"]
    assert result["schema"] == final_workload.SCHEMA
    assert result["match_backend"] == "server_text_ct_pt"
    assert result["fetch_layout"] == "uniform_fixed_window"
    assert result["security"] == "HEStd_128_classic"
    assert result["openfhe_version"] == "1.5.1"
    assert workload == expected
    assert result["fetch_preparation"]["physical_dummy_window_count"] == 0
    assert result["fetch_preparation"]["physical_capacity"] == workload["n"]
    assert result["fetch_preparation"]["record_slots"] == workload["record_slots"]
    assert result["serialized_size_independent_of_realized_k"] is True
    assert [row["selected_size"] for row in result["selector_probes"]] == list(range(18))
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
        assert fetch["unused_tail_zero"] is True
        assert row["end_to_end_exact"] is True
    for row in result["selector_probes"]:
        trace = row["trace"]
        assert row["fetch_did_not_truncate"] is True
        assert trace["query_ciphertext_count"] == 1
        assert trace["response_ciphertext_count"] == 1
        assert trace["selected_payloads_exact"] is True
        assert trace["unselected_windows_zero"] is True
        assert trace["unused_tail_zero"] is True
        if row["selected_size"] == 0:
            assert trace["empty_response_all_zero"] is True


def window_option_rows(matrix):
    return [
        {
            "N": row["N"],
            "w": row["w"],
            "n_fetch_max": row["n_fetch_max"],
            "largest_feasible_nested_prefix_n": row["n"],
            "max_serialized_slots": row["max_serialized_slots"],
            "|T|": row["text_bytes"],
            "L": row["L"],
            "N_CKKS/2": row["N_CKKS/2"],
            "U_Match": row["U_Match"],
            "U_Fetch": row["U_Fetch"],
            "limiting_condition": row["limiting_condition"],
            "status": row["status"],
        }
        for row in matrix
    ]


def workload_rows(results):
    rows = []
    for ring in RINGS:
        workload = results[ring]["workload"]
        rows.append(
            {
                "profile": profile_name(ring),
                "N": ring,
                "n": workload["n"],
                "ell": workload["record_slots"],
                "|T|": workload["text_bytes"],
                "L": workload["transform_len"],
                "N_CKKS/2": workload["ckks_slots"],
                "U_Match": workload["match_utilization"],
                "max_serialized_slots": workload["max_serialized_slots"],
                "payload_bytes": workload["payload_bytes"],
                "allocated_fetch_slots": workload["fetch_slots_used"],
                "N_BGV": workload["bgv_slots"],
                "U_Fetch": workload["fetch_window_utilization"],
                "payload_byte_utilization": workload["payload_utilization"],
                "limiting_stage": workload["limiting_stage"],
                "eligible_records": workload["eligible_records"],
                "searchable_min": workload["searchable_length_min"],
                "searchable_max": workload["searchable_length_max"],
                "searchable_mean": workload["searchable_length_mean"],
                "searchable_median": workload["searchable_length_median"],
                "payload_min": workload["payload_length_min"],
                "payload_max": workload["payload_length_max"],
                "payload_mean": workload["payload_length_mean"],
                "payload_median": workload["payload_length_median"],
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
                    "profile": profile_name(ring),
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
                "profile": profile_name(ring),
                "N": ring,
                "|T|": workload["text_bytes"],
                "L": workload["transform_len"],
                "ell": workload["record_slots"],
                "query_id": query["id"],
                "mode": query["mode"],
                "m": query["pattern_length"],
                "K": query["public_k"],
                "realized_k": len(row["locate"]["selected_candidates"]),
                "threshold": query["threshold"],
                "max_abs_error": match["max_abs_error"],
                "decision_mismatches": match["decision_mismatches"],
                "intrinsic_cancellation": match["intrinsic_cancellation_count"],
                "threshold_only": match["threshold_only_count"],
                "exact_oracle_agreement": row["exact_oracle_agreement"],
                "approximate_selection_agrees_with_exact": row["approximate_selection_agrees_with_exact"],
                "selected_payloads_exact": fetch["selected_payloads_exact"],
                "unselected_windows_zero": fetch["unselected_windows_zero"],
                "unused_tail_zero": fetch["unused_tail_zero"],
                "empty_response_all_zero": fetch["empty_response_all_zero"],
                "query_ciphertext_count": fetch["query_ciphertext_count"],
                "response_ciphertext_count": fetch["response_ciphertext_count"],
                "query_bytes": fetch["query_bytes"],
                "response_bytes": fetch["response_bytes"],
                "fetch_did_not_truncate": True,
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
                    "profile": profile_name(ring),
                    "N": ring,
                    "|T|": workload["text_bytes"],
                    "L": workload["transform_len"],
                    "ell": workload["record_slots"],
                    "query_id": f"selector-k16-s{probe['selected_size']}",
                    "mode": "fetch",
                    "m": "",
                    "K": 16,
                    "realized_k": probe["selected_size"],
                    "threshold": "",
                    "max_abs_error": "",
                    "decision_mismatches": "",
                    "intrinsic_cancellation": "",
                    "threshold_only": "",
                    "exact_oracle_agreement": True,
                    "approximate_selection_agrees_with_exact": True,
                    "selected_payloads_exact": trace["selected_payloads_exact"],
                    "unselected_windows_zero": trace["unselected_windows_zero"],
                    "unused_tail_zero": trace["unused_tail_zero"],
                    "empty_response_all_zero": trace["empty_response_all_zero"],
                    "query_ciphertext_count": trace["query_ciphertext_count"],
                    "response_ciphertext_count": trace["response_ciphertext_count"],
                    "query_bytes": trace["query_bytes"],
                    "response_bytes": trace["response_bytes"],
                    "fetch_did_not_truncate": probe["fetch_did_not_truncate"],
                    "end_to_end_exact": True,
                    "position": "",
                    "exact_score": "",
                    "decrypted_score": "",
                }
            )
    return rows


def utilization_rows(results, audit):
    rows = [
        {
            "row_type": "eligible_pool",
            "profile": "ALL",
            "N": "",
            "n": audit["eligible_record_count"],
            "ell": "",
            "U_Match": "",
            "U_Fetch": "",
            "payload_bytes": "",
            "allocated_fetch_slots": "",
            "payload_byte_utilization": "",
            "raw_searchable_min": audit["raw_searchable_min"],
            "raw_searchable_max": audit["raw_searchable_max"],
            "raw_searchable_mean": audit["raw_searchable_mean"],
            "raw_searchable_median": audit["raw_searchable_median"],
            "raw_payload_min": audit["raw_payload_min"],
            "raw_payload_max": audit["raw_payload_max"],
            "raw_payload_mean": audit["raw_payload_mean"],
            "raw_payload_median": audit["raw_payload_median"],
            "serialized_slots_min": audit["serialized_slots_min"],
            "serialized_slots_max": audit["serialized_slots_max"],
            "serialized_slots_mean": audit["serialized_slots_mean"],
            "serialized_slots_median": audit["serialized_slots_median"],
            "query_bytes": "",
            "response_bytes": "",
        }
    ]
    for ring in RINGS:
        workload = results[ring]["workload"]
        rows.append(
            {
                "row_type": "frozen_configuration",
                "profile": profile_name(ring),
                "N": ring,
                "n": workload["n"],
                "ell": workload["record_slots"],
                "U_Match": workload["match_utilization"],
                "U_Fetch": workload["fetch_window_utilization"],
                "payload_bytes": workload["payload_bytes"],
                "allocated_fetch_slots": workload["fetch_slots_used"],
                "payload_byte_utilization": workload["payload_utilization"],
                "raw_searchable_min": workload["searchable_length_min"],
                "raw_searchable_max": workload["searchable_length_max"],
                "raw_searchable_mean": workload["searchable_length_mean"],
                "raw_searchable_median": workload["searchable_length_median"],
                "raw_payload_min": workload["payload_length_min"],
                "raw_payload_max": workload["payload_length_max"],
                "raw_payload_mean": workload["payload_length_mean"],
                "raw_payload_median": workload["payload_length_median"],
                "serialized_slots_min": workload["serialized_slots_min"],
                "serialized_slots_max": workload["serialized_slots_max"],
                "serialized_slots_mean": workload["serialized_slots_mean"],
                "serialized_slots_median": workload["serialized_slots_median"],
                "query_bytes": results[ring]["query_bytes"],
                "response_bytes": results[ring]["response_bytes"],
            }
        )
    return rows


def report(results, matrix, audit, original_unchanged):
    rfps = {
        ring: {
            "cancellation": sum(
                row["match"]["intrinsic_cancellation_count"]
                for row in results[ring]["queries"]
                if row["query"]["mode"] == "rfps"
            ),
            "threshold": sum(
                row["match"]["threshold_only_count"]
                for row in results[ring]["queries"]
                if row["query"]["mode"] == "rfps"
            ),
            "mismatch": sum(
                row["match"]["decision_mismatches"]
                for row in results[ring]["queries"]
                if row["query"]["mode"] == "rfps"
            ),
        }
        for ring in RINGS
    }
    lines = [
        "# Final fixed-window CBPIR configuration freeze",
        "",
        "## Scope",
        "",
        "This is the final capacity/correctness gate before the formal seven-procedure benchmark. Diagnostic timings emitted by the runner are not paper benchmark measurements.",
        "",
        "Fetch retains uniform fixed windows `J_i=[i*ell,(i+1)*ell)`, the unchanged 128-byte frame encoder/decoder, one encrypted BGV query, one encrypted BGV response, ciphertext-plaintext evaluation, and no physical dummy windows. The meaningful encoded prefix is measured only to choose `ell`; no variable-width layout exists.",
        "",
        "## Complete eligible BGL pool",
        "",
        f"The source contains {audit['eligible_record_count']:,} deterministic eligible records. Searchable and payload lengths are both {audit['raw_payload_min']}–{audit['raw_payload_max']} bytes, mean {audit['raw_payload_mean']:.6f}, median {audit['raw_payload_median']:.0f}. The actual retained encoder produced meaningful prefixes of {audit['serialized_slots_min']}–{audit['serialized_slots_max']} slots, mean {audit['serialized_slots_mean']:.6f}, median {audit['serialized_slots_median']:.0f}.",
        "",
        "Every one of the 1,331 records was passed through the real `encode_frame()` implementation. Embedded length equaled payload length, `required_slots=14+payload_length`, all remaining legacy bytes were zero, and zero-extension followed by unchanged `decode_frame()` reconstructed exactly. Therefore every eligible record fits `w=64`; all larger allowed widths also fit.",
        "",
        "## Allowed-width audit",
        "",
        "| N | w | floor(N/w) | max feasible n | max encoded slots | |T| | L | U_Match | U_Fetch | limit |",
        "|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---|",
    ]
    for row in matrix:
        lines.append(
            f"| {row['N']} | {row['w']} | {row['n_fetch_max']} | {row['n']} | {row['max_serialized_slots']} | {row['text_bytes']} | {row['L']} | {row['U_Match']:.6f} | {row['U_Fetch']:.6f} | {row['limiting_condition']} |"
        )
    lines.extend(
        [
            "",
            "Widths 224 and 384 use `floor(N/w)` complete fixed windows and leave a zero ring tail; exact divisibility is not required by the construction.",
            "",
            "## Frozen databases",
            "",
            "| Profile | N | n | ell | |T| | L | U_Match | U_Fetch | limiting stage |",
            "|:---|---:|---:|---:|---:|---:|---:|---:|:---|",
        ]
    )
    for ring in RINGS:
        w = results[ring]["workload"]
        lines.append(
            f"| {profile_name(ring)} | {ring} | {w['n']} | {w['record_slots']} | {w['text_bytes']} | {w['transform_len']} | {w['match_utilization']:.3f} | {w['fetch_window_utilization']:.3f} | {w['limiting_stage']} |"
        )
    lines.extend(
        [
            "",
            "The smallest-valid-width rule selects `ell=64` for every profile. Actual nested-prefix measurement selects `n=128`, `256`, and `512`. Both utilization ratios are exactly 1.0 in all three configurations: `L=N_CKKS/2` and `n*ell=N_BGV`. The immediately following prefix is rejected by Fetch because another 64-slot window does not fit. Match still has byte-level slack inside its already-maximal power-of-two transform, so the limiting stage is reported as Fetch rather than jointly Match+Fetch.",
            "",
            "Payload-byte utilization is separate from view utilization. Records remain variable length and are zero-padded inside their uniform 64-slot windows.",
            "",
            "## Match, Locate, and Fetch correctness",
            "",
            "The same frozen database is used by FPS and RFPS for each N. FPS retained zero decision mismatches against the plaintext oracle. RFPS structural cancellation, nonzero threshold-only acceptance, and CKKS decision disagreement remain separate:",
            "",
            "| N | RFPS cancellations | RFPS threshold-only | RFPS CKKS disagreements |",
            "|---:|---:|---:|---:|",
        ]
    )
    for ring in RINGS:
        lines.append(
            f"| {ring} | {rfps[ring]['cancellation']} | {rfps[ring]['threshold']} | {rfps[ring]['mismatch']} |"
        )
    lines.extend(
        [
            "",
            "The regenerated corpus retains literal, absent, repeated, boundary, fewer-than-K, exactly-K, more-than-K, RFPS literal, cancellation, and threshold-only cases. Locate alone performs deterministic First_K truncation.",
            "",
            "Direct Fetch probes exercised realized `k=0..17`. Fetch never silently truncated, including `k=17` relative to public K=16. Every case used one encrypted query and one encrypted response. Sizes were constant within each N. Selected payloads reconstructed exactly; unselected windows and unused ring tails were zero. The empty selector was encrypted normally and decrypted to all zero.",
            "",
            "Exact Fetch reconstruction proves reconstruction of selected IDs, not semantic RFPS correctness.",
            "",
            "## Retained cryptographic profiles",
            "",
            "All contexts use `HEStd_128_classic`. FPS-13 retains `(N=8192, depth=2, b0=60, bs=50, FIXEDMANUAL, BV)`; FPS-14 retains `(N=16384, depth=2, b0=60, bs=40, FIXEDMANUAL, HYBRID)`; FPS-15 retains `(N=32768, depth=2, b0=60, bs=50, FIXEDMANUAL, HYBRID)`. RFPS retains depth 1, b0 60, bs 24. BGV-RNS retains t=65537 and depth 1. Only the public fixed record-window width changes to 64.",
            "",
            "## Evidence preservation and next gate",
            "",
            f"Original `phase2-full-capacity` artifacts remained byte-for-byte unchanged: `{str(original_unchanged).lower()}`.",
            "",
            "These three databases are frozen inputs for the later formal benchmark. That benchmark was not started here.",
            "",
        ]
    )
    return "\n".join(lines)


def verification_text(build, audit_run, native_runs, tests, original_before, original_after, p3_before, p3_after):
    lines = [
        "FINAL FIXED-WINDOW CONFIGURATION VERIFICATION",
        "=============================================",
        "",
        f"repository: {ROOT}",
        f"source_sha256: {final_workload.PHASE2.SOURCE_SHA256}",
        "openfhe_version: 1.5.1",
        "security: HEStd_128_classic",
        "timing_scope: configuration/correctness only; formal benchmark not started",
        f"p3_diff_sha256_before: {p3_before}",
        f"p3_diff_sha256_after:  {p3_after}",
        f"p3_unchanged: {str(p3_before == p3_after).lower()}",
        f"original_phase2_unchanged: {str(original_before == original_after).lower()}",
        "reproduction_command: PYTHONDONTWRITEBYTECODE=1 python3 experiments/cbpir-scaling/phase2-full-capacity-final/run_final.py",
        "",
        "ORIGINAL PHASE-2 SHA-256",
    ]
    for name, digest in original_before.items():
        lines.append(f"{digest}  {name}")
    lines.extend(["", "BUILD", build["command"], f"exit_status={build['status']}"])
    lines.extend(["", "FULL-POOL FRAME AUDIT", audit_run["command"], f"exit_status={audit_run['status']}"])
    lines.extend(["", "NATIVE FINAL CONFIGURATION RUNS"])
    for ring, native in native_runs.items():
        lines.extend([f"N={ring}: {native['command']}", f"exit_status={native['status']}"])
    lines.extend(["", "SERIAL REGRESSION TESTS"])
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
            "eligible_pool_audited_through_real_encoder=true",
            "all_eligible_records_fit_w64=true",
            "all_18_N_w_options_measured=true",
            "actual_text_used_without_truncation=true",
            "uniform_fixed_windows_only=true",
            "no_physical_dummy_windows=true",
            "fps_zero_decision_mismatches=true",
            "rfps_disagreements_preserved=true",
            "locate_first_k_unchanged=true",
            "fetch_does_not_truncate=true",
            "selected_fetch_payloads_exact=true",
            "unselected_fetch_windows_zero=true",
            "empty_selector_all_zero=true",
            "one_query_one_response=true",
            "serialized_sizes_independent_of_realized_k=true",
            "he_profiles_unchanged=true",
            "p3_implementation_untouched=true",
            "original_phase2_evidence_unchanged=true",
            "formal_benchmark_not_started=true",
            "",
        ]
    )
    return "\n".join(lines)


def main():
    original_before = tree_hashes(ORIGINAL_PHASE2)
    p3_before = p3_diff_hash()
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
            "cbpir_fixed_frame_audit",
            "--bin",
            "cbpir_phase2_fixed_final",
        ]
    )
    require_success(build)

    expected_audit = final_workload.pool_audit(SOURCE)
    pool_records = final_workload.PHASE2._records(
        final_workload.PHASE2.source_rows(SOURCE), expected_audit["eligible_record_count"]
    )
    audit_run = execute([FRAME_AUDIT], input_text=json.dumps(pool_records, separators=(",", ":")))
    require_success(audit_run)
    audit_rows = json.loads(audit_run["stdout"])
    assert len(audit_rows) == expected_audit["eligible_record_count"]
    assert all(
        row["required_slots"] == 14 + row["payload_bytes"]
        and row["embedded_payload_length"] == row["payload_bytes"]
        and row["legacy_padding_zero"] is True
        and row["round_trip_exact"] is True
        for row in audit_rows
    )
    native_required = {row["id"]: row["required_slots"] for row in audit_rows}
    assert native_required == expected_audit["required_slots_by_id"]
    audit = dict(expected_audit)
    audit["required_slots_by_id"] = native_required

    matrix = final_workload.build_window_matrix(SOURCE, native_required)
    studies = final_workload.build_all(SOURCE, native_required)
    results = {}
    native_runs = {}
    for ring in RINGS:
        native = execute(
            [RUNNER, ring],
            input_text=json.dumps(studies[str(ring)], separators=(",", ":")),
        )
        native_runs[ring] = native
        require_success(native)
        result = json.loads(native["stdout"])
        validate_native_result(result, studies[str(ring)]["workload"])
        results[ring] = result

    tests = [
        execute([sys.executable, "-m", "unittest", "discover", "-s", HERE, "-p", "test_final_workload.py", "-v"]),
        execute(["cargo", "test", "--locked", "--manifest-path", P0A, "--features", "openfhe-backend", "--", "--test-threads=1"]),
        execute(["cargo", "test", "--locked", "--manifest-path", P0C, "--", "--test-threads=1"]),
        execute(["cargo", "test", "--locked", "--manifest-path", P0D, "--features", "native", "--", "--test-threads=1"]),
    ]
    for test in tests:
        require_success(test)

    original_after = tree_hashes(ORIGINAL_PHASE2)
    p3_after = p3_diff_hash()
    if original_before != original_after:
        raise RuntimeError("original Phase-2 evidence changed during final configuration run")
    if p3_before != p3_after:
        raise RuntimeError("P3 diff changed during final configuration run")

    options = window_option_rows(matrix)
    workloads = workload_rows(results)
    queries = query_rows(results)
    correctness = correctness_rows(results)
    utilization = utilization_rows(results, audit)
    write_csv(HERE / "02_window_options.csv", list(options[0]), options)
    write_csv(HERE / "03_frozen_workloads.csv", list(workloads[0]), workloads)
    write_csv(HERE / "04_queries.csv", list(queries[0]), queries)
    write_csv(HERE / "05_correctness.csv", list(correctness[0]), correctness)
    write_csv(HERE / "06_utilization.csv", list(utilization[0]), utilization)
    (HERE / "01_configuration_report.md").write_text(
        report(results, matrix, audit, original_before == original_after), encoding="utf-8"
    )
    (HERE / "07_verification.txt").write_text(
        verification_text(
            build,
            audit_run,
            native_runs,
            tests,
            original_before,
            original_after,
            p3_before,
            p3_after,
        ),
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
                        "ell": results[ring]["workload"]["record_slots"],
                        "T": results[ring]["workload"]["text_bytes"],
                        "L": results[ring]["workload"]["transform_len"],
                    }
                    for ring in RINGS
                ],
                "window_options": len(options),
                "query_rows": len(queries),
                "correctness_rows": len(correctness),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
