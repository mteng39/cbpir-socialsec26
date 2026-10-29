//! Bounded Phase H synthetic K>1 experiment report.

use crate::{MatchBackend, MultiFixture, MultiPipeline, MultiRun, P0dError, multi_fixtures};
use serde_json::{Value, json};
use std::{fs, process::Command};

fn command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unavailable".into(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )
}

fn environment() -> Value {
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("model name"))
        .unwrap_or("unavailable")
        .to_owned();
    json!({
        "rustc": command("rustc", &["--version"]),
        "cargo": command("cargo", &["--version"]),
        "kernel": command("uname", &["-srmo"]),
        "cpu": cpu,
        "available_parallelism": std::thread::available_parallelism().map(usize::from).ok(),
        "omp_num_threads": std::env::var("OMP_NUM_THREADS").ok(),
        "openfhe_install_dir": std::env::var("OPENFHE_INSTALL_DIR").ok(),
        "argv": std::env::args().collect::<Vec<_>>(),
    })
}

fn summary(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    let total: u128 = values.iter().copied().map(u128::from).sum();
    #[allow(
        clippy::cast_precision_loss,
        reason = "bounded repetition counts and nanoseconds fit the report precision"
    )]
    let mean = total as f64 / values.len() as f64;
    json!({
        "samples": values.len(),
        "minimum_ns": values[0],
        "median_ns": values[values.len() / 2],
        "mean_ns": mean,
        "maximum_ns": values[values.len() - 1],
    })
}

fn ids(records: &[crate::ReconstructedRecord]) -> Vec<u64> {
    records.iter().map(|record| record.id.get()).collect()
}

fn candidate_ids(run: &MultiRun) -> Vec<u64> {
    run.locate
        .candidates
        .iter()
        .map(|candidate| candidate.id.get())
        .collect()
}

fn selected_ids(run: &MultiRun) -> Vec<u64> {
    run.locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id.get())
        .collect()
}

fn expected(fixture: &MultiFixture, run: &MultiRun) -> bool {
    let semantic_expected = fixture
        .expected_semantic_ids
        .iter()
        .copied()
        .take(fixture.public_k)
        .collect::<Vec<_>>();
    candidate_ids(run) == fixture.expected_candidate_ids
        && selected_ids(run) == fixture.expected_selected_ids
        && ids(&run.outputs) == fixture.expected_selected_ids
        && run
            .oracle
            .exact_ids
            .iter()
            .map(|id| id.get())
            .eq(fixture.expected_candidate_ids.iter().copied())
        && run
            .oracle
            .semantic_ids
            .iter()
            .map(|id| id.get())
            .eq(fixture.expected_semantic_ids.iter().copied())
        && ids(&run.oracle_semantic_outputs) == semantic_expected
        && run.locate.boundary_rejected_positions.len() == fixture.expected_boundary_rejections
        && run.locate.duplicate_count == fixture.expected_duplicate_count
        && run.locate.truncated_count == fixture.expected_truncated_count
        && run.fetch.selected_real_count == fixture.expected_selected_ids.len()
        && run.fetch.dummy_count == fixture.public_k - fixture.expected_selected_ids.len()
        && run.fetch.unselected_windows_zero
        && run.fetch_count == 1
        && run.exact_selection_agreement
        && run.semantic_selection_agreement == (fixture.expected_selected_ids == semantic_expected)
        && run
            .decisions
            .iter()
            .all(|decision| !decision.ckks_false_positive && !decision.ckks_false_negative)
}

fn failure_origin(run: &MultiRun, fixture: &MultiFixture) -> Value {
    let semantic = fixture
        .expected_semantic_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let false_selected_ids = ids(&run.outputs)
        .into_iter()
        .filter(|id| !semantic.contains(id))
        .collect::<Vec<_>>();
    json!({
        "match": {
            "ckks_false_positive_windows": run.decisions.iter().filter(|d| d.ckks_false_positive).count(),
            "ckks_false_negative_windows": run.decisions.iter().filter(|d| d.ckks_false_negative).count(),
            "intrinsic_cancellation_windows": run.decisions.iter().filter(|d| d.intrinsic_cancellation).count(),
            "threshold_only_windows": run.decisions.iter().filter(|d| d.threshold_only).count(),
        },
        "locate": {
            "boundary_rejected_positions": run.locate.boundary_rejected_positions,
            "deduplicated_positions": run.locate.duplicate_count,
        },
        "selection": {
            "candidate_count": run.locate.candidates.len(),
            "selected_count": run.locate.selected_candidates.len(),
            "truncated_count": run.locate.truncated_count,
            "false_selected_ids": false_selected_ids,
        },
        "fetch": {
            "request_count": run.fetch_count,
            "selected_real_count": run.fetch.selected_real_count,
            "dummy_count": run.fetch.dummy_count,
            "unselected_windows_zero": run.fetch.unselected_windows_zero,
            "all_selected_payloads_exact": run.outputs.iter().all(|output| fixture.records.iter().any(|source| source.id == output.id && source.payload == output.payload)),
        },
    })
}

fn run_fixture(
    fixture: &MultiFixture,
    repetitions: usize,
    backend: MatchBackend,
) -> Result<Value, P0dError> {
    let mut pipeline = MultiPipeline::prepare_with_match_backend(
        fixture.records.clone(),
        fixture.public_k,
        backend,
    )?;
    let warmup = pipeline.run(&fixture.request)?;
    let mut runs = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        runs.push(pipeline.run(&fixture.request)?);
    }
    let statistics = json!({
        "match": summary(runs.iter().map(|run| run.timings.match_ns).collect()),
        "locate": summary(runs.iter().map(|run| run.timings.locate_ns).collect()),
        "fetch": summary(runs.iter().map(|run| run.timings.fetch_ns).collect()),
        "request": summary(runs.iter().map(|run| run.timings.request_ns).collect()),
        "derived_online": summary(runs.iter().map(|run| run.timings.derived_online_sum_ns).collect()),
    });
    let all_expected = expected(fixture, &warmup) && runs.iter().all(|run| expected(fixture, run));
    let failure_origin = failure_origin(&runs[0], fixture);
    Ok(json!({
        "fixture": fixture,
        "catalog": pipeline.catalog(),
        "preparation": pipeline.preparation,
        "warmup_count": 1,
        "warmup_excluded_from_statistics": true,
        "warmup": warmup,
        "measured_repetition_count": repetitions,
        "repetitions": runs,
        "statistics_ns": statistics,
        "failure_origin": failure_origin,
        "all_expected": all_expected,
    }))
}

/// Runs the frozen K=4 synthetic matrix and retains every outcome.
#[must_use]
pub fn experiment(repetitions: usize) -> Value {
    experiment_with_match_backend(repetitions, MatchBackend::CtCt)
}

/// Runs the K=4 synthetic matrix with an explicit P0-C backend.
#[must_use]
pub fn experiment_with_match_backend(repetitions: usize, backend: MatchBackend) -> Value {
    let fixtures = multi_fixtures();
    let mut cases = Vec::new();
    let mut errors = Vec::new();
    if repetitions == 0 {
        errors.push(json!({"configuration": "repetitions must be positive"}));
    } else {
        for fixture in &fixtures {
            match run_fixture(fixture, repetitions, backend) {
                Ok(case) => cases.push(case),
                Err(error) => errors.push(json!({
                    "case": fixture.name,
                    "error": error.to_string(),
                })),
            }
        }
    }
    let passed = errors.is_empty()
        && cases.len() == fixtures.len()
        && cases.iter().all(|case| case["all_expected"] == true);
    json!({
        "schema": "p0d-kgt1-synthetic-v1",
        "classification": "bounded synthetic composition stress study",
        "match_backend": backend,
        "bounded_fixture_gate_passed": passed,
        "case_count": fixtures.len(),
        "public_k": 4,
        "measured_repetitions_per_case": repetitions,
        "software": {
            "openfhe_rs_commit": crate::report::WRAPPER_COMMIT,
            "openfhe_commit": crate::report::CORE_COMMIT,
            "openfhe_runtime": "1.5.1",
        },
        "environment": environment(),
        "cases": cases,
        "errors": errors,
        "message_sizes": {
            "classification": "joined after this native run from separately frozen CKKS and P3 serialization evidence",
            "ckks_source": "docs/project/experiments/p0d/2026-09-10/validation/ckks_sizes.json",
            "p3_source": "docs/project/experiments/p0d/2026-09-12/p3-standalone-validation/p3_report.json",
        },
        "limits": [
            "K=4 synthetic fixtures only; K=8 and K=16 are not part of this rate or timing matrix",
            "synthetic cases are selected stress mechanisms, not deployment-frequency samples",
            "RFPS baseline semantics are preserved; false candidates are not corrected",
            "local cohosted harness; no network, malicious-server, ORAM, or side-channel claim",
            "exact Fetch reconstruction does not imply semantically correct Match selection",
        ],
    })
}
