use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use cbpir_search::{
    CiphertextState, CkksError, CkksRunMetrics, DecisionCause, EncryptedFpsResult,
    EncryptedRfpsResult, MAX_CKKS_BATCH_SIZE, MAX_PATTERN_LEN, PHASE1_FIXTURE_TEXT_LEN,
    PHASE1_FIXTURE_TRANSFORM_LEN, ProposalWorkloads, RFPS_SCALING_MODULUS_BITS,
    RfpsPackedTrialInput, ScoreDecision, SerializedArtifactSizes, ckks_fps,
    ckks_fps_serialization_roundtrip, ckks_rfps, ckks_rfps_packed_trials,
    ckks_rfps_serialization_roundtrip, classify_score, fixture_sha256, proposal_workloads,
};
use serde_json::{Value, json};

const OPENFHE_RS_COMMIT: &str = "3e6a46b1377e89e09414cbca7c0c0140c5338397";
const OPENFHE_COMMIT: &str = "1306d14f8c26bb6150d3e6ad54f28dfe1007689e";
const AUTHORITY_SHA256: &str = "0f1a72d6fca44095cc8a24fd8447387e79f6f5b4e07901391e9b74701f6f4f87";
const EXPECTED_FIXTURE_SHA256: &str =
    "5c3ac1b262acd67f9b64e3147e5e6ba9e2c54b7bcc484caedabde4a1d775bd77";

fn decision_name(cause: DecisionCause) -> &'static str {
    match cause {
        DecisionCause::TrueMatchAccepted => "true_match_accepted",
        DecisionCause::TrueMatchRejectedByCkks => "true_match_rejected_by_ckks",
        DecisionCause::MismatchRejected => "mismatch_rejected",
        DecisionCause::IntrinsicCancellation => "intrinsic_random_cancellation",
        DecisionCause::ThresholdOnly => "threshold_only",
        DecisionCause::CkksFalsePositiveThresholdCrossing => {
            "ckks_false_positive_threshold_crossing"
        }
        DecisionCause::CkksFalseNegativeThresholdCrossing => {
            "ckks_false_negative_threshold_crossing"
        }
    }
}

fn state_json(state: CiphertextState) -> Value {
    json!({
        "level": state.level,
        "noise_scale_degree": state.noise_scale_degree,
        "scaling_factor": state.scaling_factor,
        "component_count": state.component_count,
        "evidence_label": "OpenFHE ciphertext metadata; not residual noise budget"
    })
}

fn metrics_json(metrics: &CkksRunMetrics) -> Value {
    let timings = metrics
        .phase_timings
        .expect("FPS/RFPS experiment run has phase timings");
    json!({
        "ring_dimension": metrics.ring_dimension,
        "batch_size": metrics.batch_size,
        "multiplicative_depth_configured": metrics.multiplicative_depth,
        "scaling_modulus_bits": metrics.scaling_modulus_bits,
        "first_modulus_bits": metrics.first_modulus_bits,
        "scaling_technique": metrics.scaling_technique,
        "reported_log_precision_bits": metrics.reported_log_precision_bits,
        "reported_log_precision_note": "OpenFHE 1.5.1 does not implement GetLogPrecision for COMPLEX CKKS; independent decoded error is reported instead",
        "input_ciphertext_state": state_json(metrics.input_state),
        "output_ciphertext_state": state_json(metrics.output_state),
        "operations": {
            "ciphertext_multiplications": metrics.ciphertext_multiplications,
            "ciphertext_plaintext_multiplications": metrics.ciphertext_plaintext_multiplications,
            "ciphertext_constant_multiplications": metrics.ciphertext_constant_multiplications,
            "additions": metrics.additions,
            "subtractions": metrics.subtractions,
            "relinearizations": metrics.relinearizations,
            "explicit_rescaling_operations": metrics.explicit_rescaling_operations
        },
        "phase_timings_ns": {
            "context_and_key_generation": timings.context_and_key_generation_ns,
            "client_setup": timings.client_setup_ns,
            "client_query": timings.client_query_ns,
            "server_evaluation": timings.server_evaluation_ns,
            "client_decrypt_idft": timings.client_decrypt_idft_ns
        }
    })
}

fn fps_json(result: &EncryptedFpsResult) -> Value {
    json!({
        "scores": result.scores,
        "exact_scores": result.exact_scores,
        "matching_positions": result.matching_positions,
        "max_abs_error": result.max_abs_error,
        "decision_threshold": 0.5,
        "metrics": metrics_json(&result.metrics)
    })
}

fn rfps_json(result: &EncryptedRfpsResult, threshold: f64) -> Value {
    json!({
        "scores": result.scores,
        "exact_scores": result.exact_scores,
        "matching_positions": result.matching_positions,
        "max_abs_error": result.max_abs_error,
        "decision_threshold": threshold,
        "decisions": result.decisions.iter().map(|decision| json!({
            "position": decision.position,
            "exact_score": decision.exact_score,
            "approximate_score": decision.approximate_score,
            "window_matches": decision.window_matches,
            "cause": decision_name(decision.cause)
        })).collect::<Vec<_>>(),
        "metrics": metrics_json(&result.metrics)
    })
}

fn exact_fps_scores(text: &[u8], pattern: &[u8]) -> Vec<i64> {
    text.windows(pattern.len())
        .map(|window| {
            window
                .iter()
                .zip(pattern)
                .map(|(text_value, pattern_value)| {
                    let difference = i64::from(*text_value) - i64::from(*pattern_value);
                    difference * difference
                })
                .sum()
        })
        .collect()
}

fn reclassify_rfps(
    text: &[u8],
    pattern: &[u8],
    result: &EncryptedRfpsResult,
    threshold: f64,
) -> Vec<ScoreDecision> {
    result
        .exact_scores
        .iter()
        .copied()
        .zip(result.scores.iter().copied())
        .enumerate()
        .map(|(position, (exact_score, approximate_score))| {
            let window_matches = text[position..position + pattern.len()] == *pattern;
            ScoreDecision {
                position,
                exact_score,
                approximate_score,
                window_matches,
                cause: classify_score(window_matches, exact_score, approximate_score, threshold),
            }
        })
        .collect()
}

fn classification_counts(decisions: &[ScoreDecision]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::from([
        ("ckks_false_negative_threshold_crossing", 0),
        ("ckks_false_positive_threshold_crossing", 0),
        ("intrinsic_random_cancellation", 0),
        ("mismatch_rejected", 0),
        ("threshold_only", 0),
        ("true_match_accepted", 0),
        ("true_match_rejected_by_ckks", 0),
    ]);
    for decision in decisions {
        *counts
            .get_mut(decision_name(decision.cause))
            .expect("all decision causes have a report key") += 1;
    }
    counts
}

fn crossing_count(decisions: &[ScoreDecision]) -> usize {
    decisions
        .iter()
        .filter(|decision| {
            matches!(
                decision.cause,
                DecisionCause::CkksFalsePositiveThresholdCrossing
                    | DecisionCause::CkksFalseNegativeThresholdCrossing
                    | DecisionCause::TrueMatchRejectedByCkks
            )
        })
        .count()
}

fn decision_regime_json(decisions: &[ScoreDecision], threshold: f64) -> Value {
    let matching_positions = decisions
        .iter()
        .filter_map(|decision| {
            (decision.approximate_score.abs() <= threshold).then_some(decision.position)
        })
        .collect::<Vec<_>>();
    let accepted_mismatches = decisions
        .iter()
        .filter(|decision| {
            !decision.window_matches && decision.approximate_score.abs() <= threshold
        })
        .map(|decision| {
            json!({
                "position": decision.position,
                "exact_score": decision.exact_score,
                "approximate_score": decision.approximate_score,
                "cause": decision_name(decision.cause)
            })
        })
        .collect::<Vec<_>>();
    json!({
        "threshold": threshold,
        "matching_positions": matching_positions,
        "classification_counts": classification_counts(decisions),
        "classified_score_count": decisions.len(),
        "accepted_mismatches": accepted_mismatches
    })
}

#[allow(clippy::cast_precision_loss)] // Declared FPS exact scores are below 2^53.
fn workload_case_json(
    id: &str,
    kind: &str,
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    expected_positions: &[usize],
    fixture_metadata: &Value,
) -> (Value, usize) {
    let fps = ckks_fps(text, pattern, 0.5).expect("validated fixture FPS workload");
    let rfps = ckks_rfps(text, pattern, randomizer, 0.5).expect("validated fixture RFPS workload");
    let exact_fps = exact_fps_scores(text, pattern);
    let fps_internal_exact_agreement = fps
        .exact_scores
        .iter()
        .zip(&exact_fps)
        .all(|(internal, independent)| (*internal - *independent as f64).abs() <= f64::EPSILON);
    let decisions_at_0_5 = reclassify_rfps(text, pattern, &rfps, 0.5);
    let decisions_at_4_0 = reclassify_rfps(text, pattern, &rfps, 4.0);
    let fps_true_matches_preserved = expected_positions
        .iter()
        .all(|position| fps.matching_positions.contains(position));
    let rfps_true_matches_preserved_at_0_5 = expected_positions
        .iter()
        .all(|position| rfps.matching_positions.contains(position));
    let accepted_fps_mismatches = fps
        .matching_positions
        .iter()
        .copied()
        .filter(|position| text[*position..*position + pattern.len()] != *pattern)
        .collect::<Vec<_>>();
    let diagnostic_crossings = crossing_count(&decisions_at_0_5);

    (
        json!({
            "id": id,
            "kind": kind,
            "text_len": text.len(),
            "pattern_len": pattern.len(),
            "transform_len": fps.metrics.batch_size,
            "expected_positions": expected_positions,
            "exact_fps_scores": exact_fps,
            "exact_rfps_scores": rfps.exact_scores,
            "approximate_fps_scores": fps.scores,
            "approximate_rfps_scores": rfps.scores,
            "fps_matching_positions_at_0_5": fps.matching_positions,
            "fps_accepted_mismatches_at_0_5": accepted_fps_mismatches,
            "fps_internal_exact_agreement": fps_internal_exact_agreement,
            "fps_true_matches_preserved_at_0_5": fps_true_matches_preserved,
            "rfps_true_matches_preserved_at_0_5": rfps_true_matches_preserved_at_0_5,
            "fps_max_abs_error": fps.max_abs_error,
            "rfps_max_abs_error": rfps.max_abs_error,
            "rfps_decision_regimes": {
                "diagnostic_0_5": decision_regime_json(&decisions_at_0_5, 0.5),
                "source_4_0": decision_regime_json(&decisions_at_4_0, 4.0)
            },
            "fps_metrics": metrics_json(&fps.metrics),
            "rfps_metrics": metrics_json(&rfps.metrics),
            "fixture_metadata": fixture_metadata
        }),
        diagnostic_crossings,
    )
}

fn serialization_json(
    sizes: &SerializedArtifactSizes,
    reload_max_abs_error: f64,
    reload_matching_positions: &[usize],
) -> Value {
    json!({
        "format": "OpenFHE binary serialization",
        "context_bytes": sizes.context_bytes,
        "public_key_bytes": sizes.public_key_bytes,
        "private_key_bytes": sizes.private_key_bytes,
        "eval_mult_key_bytes": sizes.eval_mult_key_bytes,
        "setup_ciphertext_bytes": sizes.setup_ciphertext_bytes,
        "query_ciphertext_bytes": sizes.query_ciphertext_bytes,
        "response_ciphertext_bytes": sizes.response_ciphertext_bytes,
        "reload_max_abs_error": reload_max_abs_error,
        "reload_matching_positions": reload_matching_positions,
        "measurement": "filesystem metadata length of each serialized file"
    })
}

fn process_peak_rss_kib() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        })
        .unwrap_or(0)
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|cpuinfo| {
            cpuinfo.lines().find_map(|line| {
                line.strip_prefix("model name\t:")
                    .map(str::trim)
                    .map(ToOwned::to_owned)
            })
        })
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn memory_total_kib() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| {
            meminfo.lines().find_map(|line| {
                line.strip_prefix("MemTotal:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        })
        .unwrap_or(0)
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(
            || "unavailable".to_owned(),
            |version| version.trim().to_owned(),
        )
}

fn locked_version(crate_name: &str) -> String {
    let mut current_name = None;
    for line in include_str!("../../../../Cargo.lock").lines() {
        if line == "[[package]]" {
            current_name = None;
        } else if let Some(name) = line
            .strip_prefix("name = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            current_name = Some(name);
        } else if current_name == Some(crate_name)
            && let Some(version) = line
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
        {
            return version.to_owned();
        }
    }
    "unavailable".to_owned()
}

fn benchmark_case(length: usize) -> Value {
    let text = (0..length)
        .map(|index| u8::try_from(index % 251).expect("bounded byte"))
        .collect::<Vec<_>>();
    let pattern_len = length.min(8);
    let pattern = text[length - pattern_len..].to_vec();
    let randomizer = (0..pattern_len)
        .map(|index| u8::try_from(index + 3).expect("bounded randomizer"))
        .collect::<Vec<_>>();
    let fps = ckks_fps(&text, &pattern, 0.5).expect("valid benchmark FPS");
    let rfps = ckks_rfps(&text, &pattern, &randomizer, 0.5).expect("valid benchmark RFPS");
    let rfps_ckks_threshold_crossings = rfps
        .decisions
        .iter()
        .filter(|decision| {
            matches!(
                decision.cause,
                DecisionCause::CkksFalsePositiveThresholdCrossing
                    | DecisionCause::CkksFalseNegativeThresholdCrossing
                    | DecisionCause::TrueMatchRejectedByCkks
            )
        })
        .count();
    let expected_matches = vec![length - pattern_len];
    json!({
        "text_len": length,
        "pattern_len": pattern_len,
        "expected_last_match": length - pattern_len,
        "fps_matching_positions": fps.matching_positions,
        "fps_decision_matches_exact_oracle": fps.matching_positions == expected_matches,
        "fps_max_abs_error": fps.max_abs_error,
        "fps_phase_timings_ns": metrics_json(&fps.metrics)["phase_timings_ns"],
        "rfps_matching_positions": rfps.matching_positions,
        "rfps_decision_matches_exact_oracle": rfps.matching_positions == expected_matches,
        "rfps_max_abs_error": rfps.max_abs_error,
        "rfps_ckks_threshold_crossings": rfps_ckks_threshold_crossings,
        "rfps_phase_timings_ns": metrics_json(&rfps.metrics)["phase_timings_ns"],
        "sampling_note": "one observed run per protocol and size with RFPS coordinates 3..=10; not a probability sample or cross-system benchmark"
    })
}

fn domain_boundary_case() -> Value {
    let maximum = vec![u8::MAX; PHASE1_FIXTURE_TEXT_LEN];
    let maximum_pattern = vec![u8::MAX; MAX_PATTERN_LEN];
    let zero = vec![0_u8; MAX_PATTERN_LEN];
    let randomizer = vec![32_u8; MAX_PATTERN_LEN];
    let expected_positions = (0..=PHASE1_FIXTURE_TEXT_LEN - MAX_PATTERN_LEN).collect::<Vec<_>>();
    let fps_true = ckks_fps(&maximum, &maximum_pattern, 0.5).expect("extreme FPS true match");
    let fps_mismatch = ckks_fps(&maximum, &zero, 0.5).expect("extreme FPS mismatch");
    let rfps_true =
        ckks_rfps(&maximum, &maximum_pattern, &randomizer, 0.5).expect("extreme RFPS true match");
    let rfps_mismatch =
        ckks_rfps(&maximum, &zero, &randomizer, 0.5).expect("extreme RFPS mismatch");

    json!({
        "configuration": {
            "text_len": PHASE1_FIXTURE_TEXT_LEN,
            "pattern_len": MAX_PATTERN_LEN,
            "symbol_extremes": [0, 255],
            "rfps_randomizer": 32,
            "fps_threshold": 0.5,
            "rfps_diagnostic_threshold": 0.5,
            "rfps_source_threshold": 4.0,
            "scope": "four extremal aligned vectors; not exhaustive coverage of the full Cartesian input domain"
        },
        "fps_true_match": {
            "exact_score": fps_true.exact_scores[0],
            "approximate_score": fps_true.scores[0],
            "absolute_error": fps_true.max_abs_error,
            "decision_preserved": fps_true.matching_positions == expected_positions
        },
        "fps_max_mismatch": {
            "exact_score": fps_mismatch.exact_scores[0],
            "approximate_score": fps_mismatch.scores[0],
            "absolute_error": fps_mismatch.max_abs_error,
            "decision_preserved": fps_mismatch.matching_positions.is_empty()
        },
        "rfps_true_match": {
            "exact_score": rfps_true.exact_scores[0],
            "approximate_score": rfps_true.scores[0],
            "absolute_error": rfps_true.max_abs_error,
            "decision_preserved": rfps_true.matching_positions == expected_positions
        },
        "rfps_max_mismatch": {
            "exact_score": rfps_mismatch.exact_scores[0],
            "approximate_score": rfps_mismatch.scores[0],
            "absolute_error": rfps_mismatch.max_abs_error,
            "decision_preserved": rfps_mismatch.matching_positions.is_empty()
        }
    })
}

fn matched_workload_reports(fixture: &ProposalWorkloads) -> (Value, Vec<Value>, Vec<Value>, usize) {
    let mut diagnostic_crossings = 0_usize;
    let synthetic = fixture
        .synthetic_cases
        .iter()
        .map(|case| {
            let (report, crossings) = workload_case_json(
                &case.id,
                &case.kind,
                &case.text,
                &case.pattern,
                &case.randomizer,
                &case.expected_match_positions,
                &json!({
                    "designated_position": case.designated_position,
                    "mismatch_count": case.mismatch_count
                }),
            );
            diagnostic_crossings = diagnostic_crossings.saturating_add(crossings);
            report
        })
        .collect::<Vec<_>>();
    let bgl_cases = fixture
        .bgl
        .cases
        .iter()
        .map(|case| {
            let (report, crossings) = workload_case_json(
                &case.id,
                "bgl",
                &case.text,
                &case.pattern,
                &case.randomizer,
                &case.expected_match_positions,
                &json!({
                    "source_line": case.source_line,
                    "stable_record_id": case.id,
                    "pattern_offset": case.pattern_offset
                }),
            );
            diagnostic_crossings = diagnostic_crossings.saturating_add(crossings);
            report
        })
        .collect::<Vec<_>>();
    let source = &fixture.bgl.source;
    let bgl_source = json!({
        "repository": source.repository,
        "commit": source.commit,
        "path": source.path,
        "sha256": source.sha256,
        "size_bytes": source.size_bytes,
        "record_count": source.record_count,
        "terminal_newline": source.terminal_newline,
        "license_path": source.license_path,
        "license_sha256": source.license_sha256,
        "license_size_bytes": source.license_size_bytes
    });
    (bgl_source, synthetic, bgl_cases, diagnostic_crossings)
}

#[derive(Default)]
struct TimingTotals {
    context_and_key_generation: u64,
    client_setup: u64,
    client_query: u64,
    server_evaluation: u64,
    client_decrypt_idft: u64,
}

impl TimingTotals {
    fn add(&mut self, metrics: &CkksRunMetrics) {
        let timings = metrics
            .phase_timings
            .expect("packed RFPS run has phase timings");
        self.context_and_key_generation = self
            .context_and_key_generation
            .saturating_add(timings.context_and_key_generation_ns);
        self.client_setup = self.client_setup.saturating_add(timings.client_setup_ns);
        self.client_query = self.client_query.saturating_add(timings.client_query_ns);
        self.server_evaluation = self
            .server_evaluation
            .saturating_add(timings.server_evaluation_ns);
        self.client_decrypt_idft = self
            .client_decrypt_idft
            .saturating_add(timings.client_decrypt_idft_ns);
    }

    fn as_json(&self) -> Value {
        json!({
            "context_and_key_generation": self.context_and_key_generation,
            "client_setup": self.client_setup,
            "client_query": self.client_query,
            "server_evaluation": self.server_evaluation,
            "client_decrypt_idft": self.client_decrypt_idft
        })
    }
}

struct PackedOutcome {
    id: String,
    exact_score: i64,
    approximate_score: f64,
    window_matches: bool,
    cause_at_0_5: DecisionCause,
    cause_at_4_0: DecisionCause,
}

impl PackedOutcome {
    fn as_json(&self) -> Value {
        json!({
            "id": self.id,
            "exact_score": self.exact_score,
            "approximate_score": self.approximate_score,
            "window_matches": self.window_matches,
            "cause_at_0_5": decision_name(self.cause_at_0_5),
            "cause_at_4_0": decision_name(self.cause_at_4_0)
        })
    }
}

struct PackedRun {
    outcomes: Vec<PackedOutcome>,
    batches: Vec<Value>,
    timing_totals: TimingTotals,
    max_abs_error: f64,
}

fn run_packed_inputs(
    kind: &str,
    mismatch_count: usize,
    inputs: &[RfpsPackedTrialInput],
    window_matches: bool,
) -> PackedRun {
    let block_transform_len = (mismatch_count + 1).next_power_of_two();
    let trials_per_batch = MAX_CKKS_BATCH_SIZE / block_transform_len;
    let mut outcomes = Vec::with_capacity(inputs.len());
    let mut batches = Vec::new();
    let mut timing_totals = TimingTotals::default();
    let mut max_abs_error = 0.0_f64;

    for (batch_index, batch_inputs) in inputs.chunks(trials_per_batch).enumerate() {
        let result = ckks_rfps_packed_trials(batch_inputs, 0.5)
            .expect("validated fixture packed RFPS workload");
        assert_eq!(result.block_transform_len, block_transform_len);
        timing_totals.add(&result.metrics);
        max_abs_error = max_abs_error.max(result.max_abs_error);
        batches.push(json!({
            "id": format!("{kind}-k{mismatch_count}-batch-{:02}", batch_index + 1),
            "trial_count": batch_inputs.len(),
            "block_transform_len": result.block_transform_len,
            "encoded_slot_count": result.encoded_slot_count,
            "padding_slot_count": result.padding_slot_count,
            "padded_slot_count": result.padded_slot_count,
            "max_abs_error": result.max_abs_error,
            "metrics": metrics_json(&result.metrics)
        }));
        outcomes.extend(result.trials.into_iter().map(|trial| PackedOutcome {
            cause_at_0_5: classify_score(
                window_matches,
                trial.exact_score,
                trial.approximate_score,
                0.5,
            ),
            cause_at_4_0: classify_score(
                window_matches,
                trial.exact_score,
                trial.approximate_score,
                4.0,
            ),
            id: trial.id,
            exact_score: trial.exact_score,
            approximate_score: trial.approximate_score,
            window_matches,
        }));
    }
    PackedRun {
        outcomes,
        batches,
        timing_totals,
        max_abs_error,
    }
}

fn cause_counts<I>(causes: I) -> BTreeMap<&'static str, usize>
where
    I: IntoIterator<Item = DecisionCause>,
{
    let mut counts = BTreeMap::from([
        ("ckks_false_negative_threshold_crossing", 0),
        ("ckks_false_positive_threshold_crossing", 0),
        ("intrinsic_random_cancellation", 0),
        ("mismatch_rejected", 0),
        ("threshold_only", 0),
        ("true_match_accepted", 0),
        ("true_match_rejected_by_ckks", 0),
    ]);
    for cause in causes {
        *counts
            .get_mut(decision_name(cause))
            .expect("all decision causes have a report key") += 1;
    }
    counts
}

fn theory_counts(mismatch_count: usize) -> (u64, u64) {
    match mismatch_count {
        1 => (0, 180),
        2 => (412, 32_400),
        3 => (47_052, 5_832_000),
        4 => (7_573_724, 1_049_760_000),
        _ => unreachable!("fixture limits mismatch count to 1..=4"),
    }
}

#[allow(clippy::cast_precision_loss)] // Theory counts are exactly representable at this size.
#[allow(clippy::too_many_lines)] // Keep each probability field beside its matched packed evidence.
fn rfps_probability_report(fixture: &ProposalWorkloads) -> (Vec<Value>, usize) {
    let mut reports = Vec::new();
    let mut diagnostic_crossings = 0_usize;

    for mismatch_count in 1..=4 {
        let trial_inputs = fixture
            .rfps_probability
            .trials
            .iter()
            .filter(|trial| trial.k == mismatch_count)
            .map(|trial| RfpsPackedTrialInput {
                id: trial.id.clone(),
                text: trial.text.clone(),
                pattern: trial.pattern.clone(),
                randomizer: trial.randomizer.clone(),
            })
            .collect::<Vec<_>>();
        let control_inputs = fixture
            .rfps_probability
            .true_match_controls
            .iter()
            .filter(|control| control.k == mismatch_count)
            .map(|control| RfpsPackedTrialInput {
                id: control.id.clone(),
                text: control.text.clone(),
                pattern: control.pattern.clone(),
                randomizer: control.randomizer.clone(),
            })
            .collect::<Vec<_>>();
        let trial_run = run_packed_inputs("mismatch", mismatch_count, &trial_inputs, false);
        let control_run = run_packed_inputs("true-control", mismatch_count, &control_inputs, true);
        let exact_zero_count = trial_run
            .outcomes
            .iter()
            .filter(|outcome| outcome.exact_score == 0)
            .count();
        let diagnostic_counts = cause_counts(
            trial_run
                .outcomes
                .iter()
                .map(|outcome| outcome.cause_at_0_5),
        );
        let source_counts = cause_counts(
            trial_run
                .outcomes
                .iter()
                .map(|outcome| outcome.cause_at_4_0),
        );
        let control_diagnostic_counts = cause_counts(
            control_run
                .outcomes
                .iter()
                .map(|outcome| outcome.cause_at_0_5),
        );
        let control_source_counts = cause_counts(
            control_run
                .outcomes
                .iter()
                .map(|outcome| outcome.cause_at_4_0),
        );
        diagnostic_crossings = diagnostic_crossings.saturating_add(
            trial_run
                .outcomes
                .iter()
                .filter(|outcome| {
                    matches!(
                        outcome.cause_at_0_5,
                        DecisionCause::CkksFalsePositiveThresholdCrossing
                            | DecisionCause::CkksFalseNegativeThresholdCrossing
                    )
                })
                .count(),
        );
        diagnostic_crossings = diagnostic_crossings.saturating_add(
            control_run
                .outcomes
                .iter()
                .filter(|outcome| outcome.cause_at_0_5 == DecisionCause::TrueMatchRejectedByCkks)
                .count(),
        );
        let (theoretical_zero_count, theoretical_total_count) = theory_counts(mismatch_count);
        reports.push(json!({
            "mismatch_count": mismatch_count,
            "random_variables": fixture.rfps_probability.model.score,
            "conditioning": fixture.rfps_probability.model.conditioning_event,
            "alphabet": fixture.rfps_probability.model.alphabet,
            "ordered_distinct_symbol_pairs": fixture.rfps_probability.model.ordered_distinct_pairs,
            "pair_distribution": fixture.rfps_probability.model.pair_sampling,
            "randomizer_support": fixture.rfps_probability.model.randomizer_domain,
            "randomizer_distribution": fixture.rfps_probability.model.randomizer_sampling,
            "independence_assumption": fixture.rfps_probability.model.independence,
            "algebraic_domain": fixture.rfps_probability.model.algebraic_domain,
            "p0b_theory_reference": {
                "path": "docs/project/experiments/search/2026-09-06/p0b-proposal-workloads/rfps_probability_derivation.md",
                "theoretical_zero_count": theoretical_zero_count.to_string(),
                "theoretical_total_count": theoretical_total_count.to_string(),
                "theoretical_probability": theoretical_zero_count as f64 / theoretical_total_count as f64,
                "status": "imported comparison target; not independently re-derived by P0-C"
            },
            "fixture_trials": trial_run.outcomes.len(),
            "exact_zero_count": exact_zero_count,
            "exact_zero_probability": exact_zero_count as f64 / trial_run.outcomes.len() as f64,
            "packed_batch_count": trial_run.batches.len(),
            "expected_packed_batch_count": match mismatch_count { 1 => 2, 2 | 3 => 4, 4 => 8, _ => unreachable!() },
            "max_abs_error": trial_run.max_abs_error,
            "classification_counts_at_0_5": diagnostic_counts,
            "classification_counts_at_4_0": source_counts,
            "aggregate_phase_timings_ns": trial_run.timing_totals.as_json(),
            "batches": trial_run.batches,
            "trial_outcomes": trial_run.outcomes.iter().map(PackedOutcome::as_json).collect::<Vec<_>>(),
            "true_match_control_count": control_run.outcomes.len(),
            "true_match_controls_in_probability_denominator": false,
            "true_match_control_batch_count": control_run.batches.len(),
            "true_match_control_max_abs_error": control_run.max_abs_error,
            "true_match_control_classification_counts_at_0_5": control_diagnostic_counts,
            "true_match_control_classification_counts_at_4_0": control_source_counts,
            "true_match_control_aggregate_phase_timings_ns": control_run.timing_totals.as_json(),
            "true_match_control_batches": control_run.batches,
            "true_match_controls": control_run.outcomes.iter().map(PackedOutcome::as_json).collect::<Vec<_>>()
        }));
    }
    (reports, diagnostic_crossings)
}

fn maximum_workload_serialization(fixture: &ProposalWorkloads) -> Value {
    let case = fixture
        .synthetic_cases
        .iter()
        .find(|case| case.id == "synthetic-n1024-m64-exact")
        .expect("canonical maximum workload case");
    let fps = ckks_fps_serialization_roundtrip(&case.text, &case.pattern, 0.5)
        .expect("maximum FPS serialization round trip");
    let rfps = ckks_rfps_serialization_roundtrip(&case.text, &case.pattern, &case.randomizer, 0.5)
        .expect("maximum RFPS serialization round trip");
    let fps_preserved = case
        .expected_match_positions
        .iter()
        .all(|position| fps.matching_positions.contains(position));
    let rfps_preserved = case
        .expected_match_positions
        .iter()
        .all(|position| rfps.matching_positions.contains(position));
    json!({
        "case_id": case.id,
        "text_len": case.text_len,
        "pattern_len": case.pattern_len,
        "expected_positions": case.expected_match_positions,
        "fps": {
            "artifacts": serialization_json(&fps.sizes, fps.max_abs_error, &fps.matching_positions),
            "reload_preserved_expected_positions": fps_preserved
        },
        "rfps": {
            "artifacts": serialization_json(&rfps.sizes, rfps.max_abs_error, &rfps.matching_positions),
            "reload_preserved_expected_positions": rfps_preserved
        }
    })
}

#[allow(clippy::too_many_lines)] // Keep the complete evidence schema visible in one place.
fn main() {
    let quick = std::env::args().any(|argument| argument == "--quick");
    let fixture = proposal_workloads().expect("canonical proposal workload fixture");
    assert_eq!(fixture_sha256(), EXPECTED_FIXTURE_SHA256);
    let fixed_text = [1, 2, 3, 4, 1, 2, 3];
    let fixed_pattern = [2, 3, 4];
    let fixed_randomizer = [2, 5, 3];

    let fps = ckks_fps(&fixed_text, &fixed_pattern, 0.5).expect("fixed FPS run");
    let rfps =
        ckks_rfps(&fixed_text, &fixed_pattern, &fixed_randomizer, 0.5).expect("fixed RFPS run");
    let intrinsic =
        ckks_rfps(&[2, 0, 4], &[0, 1, 3], &[2, 5, 1], 0.5).expect("intrinsic-cancellation witness");
    let threshold_stress =
        ckks_rfps(&[2], &[0], &[1], 4.0).expect("threshold-only acceptance witness");
    let fps_serialized = ckks_fps_serialization_roundtrip(&fixed_text, &fixed_pattern, 0.5)
        .expect("FPS serialization round trip");
    let rfps_serialized =
        ckks_rfps_serialization_roundtrip(&fixed_text, &fixed_pattern, &fixed_randomizer, 0.5)
            .expect("RFPS serialization round trip");
    let maximum_serialization = maximum_workload_serialization(&fixture);

    let lengths: &[usize] = if quick { &[8] } else { &[8, 32, 64, 128] };
    let benchmarks = lengths
        .iter()
        .copied()
        .map(benchmark_case)
        .collect::<Vec<_>>();
    let benchmark_ckks_crossings = benchmarks
        .iter()
        .filter_map(|benchmark| benchmark["rfps_ckks_threshold_crossings"].as_u64())
        .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
        .sum::<usize>();
    let (bgl_source, synthetic_matrix, bgl_cases, matrix_ckks_crossings) =
        matched_workload_reports(&fixture);
    let (rfps_probability, probability_ckks_crossings) = rfps_probability_report(&fixture);
    let ckks_crossings = rfps
        .decisions
        .iter()
        .chain(intrinsic.decisions.iter())
        .chain(threshold_stress.decisions.iter())
        .filter(|decision| {
            matches!(
                decision.cause,
                DecisionCause::CkksFalsePositiveThresholdCrossing
                    | DecisionCause::CkksFalseNegativeThresholdCrossing
                    | DecisionCause::TrueMatchRejectedByCkks
            )
        })
        .count()
        .saturating_add(benchmark_ckks_crossings)
        .saturating_add(matrix_ckks_crossings)
        .saturating_add(probability_ckks_crossings);

    let invalid_threshold = matches!(
        ckks_fps(&[1], &[1], -1.0),
        Err(CkksError::InvalidDecisionThreshold)
    );
    let over_capacity_text = vec![0_u8; 8_192];
    let over_capacity = matches!(
        ckks_fps(&over_capacity_text, &[0], 0.5),
        Err(CkksError::TransformExceedsProfileCapacity {
            transform_len: 16_384,
            available_slots: 8_192
        })
    );
    let unsupported_transform = matches!(
        cbpir_search::CkksConfig::for_transform_len(3),
        Err(CkksError::UnsupportedTransformLength(3))
    );

    let report = json!({
        "schema": "p0c-experiment-v3",
        "run_date_utc": "2026-09-08",
        "quick": quick,
        "authority": {
            "path": "docs/proposal/proposal_upd.pdf",
            "sha256": AUTHORITY_SHA256
        },
        "workload_fixture": {
            "path": "data/workloads/search_workloads_v1.json",
            "sha256": fixture_sha256(),
            "schema": fixture.schema,
            "synthetic_case_count": fixture.synthetic_cases.len(),
            "rfps_mismatch_trial_count": fixture.rfps_probability.trials.len(),
            "rfps_true_match_control_count": fixture.rfps_probability.true_match_controls.len(),
            "bgl_case_count": fixture.bgl.cases.len()
        },
        "source_construction": {
            "principal_source": "literature/1. private_search/feer_2024_symmetric_search_he-masters-thesis_aurel-feer(ethzurich).pdf",
            "fps": "equation (4.6), PDF p. 33; protocol roles PDF pp. 33-34",
            "rfps": "equation (4.10), PDF p. 37; protocol roles PDF pp. 37-38",
            "implementation_parameters": "PDF pp. 59-61",
            "reproduction_label": "independent OpenFHE translation of documented construction; Feer's implementation used Microsoft SEAL"
        },
        "software": {
            "crate": format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
            "rustc": rustc_version(),
            "openfhe_version": fps.metrics.openfhe_version,
            "openfhe_rs_repository": "https://github.com/iasenovets/openfhe-rs",
            "openfhe_rs_commit": OPENFHE_RS_COMMIT,
            "openfhe_rs_crate_version": locked_version("openfhe"),
            "openfhe_commit": OPENFHE_COMMIT,
            "num_complex": locked_version("num-complex"),
            "serde_json": locked_version("serde_json")
        },
        "hardware_environment": {
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "cpu_model": cpu_model(),
            "logical_cpus": std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get),
            "memory_total_kib": memory_total_kib()
        },
        "config": {
            "symbol_domain": [0, 255],
            "randomizer_api_domain": [1, 32],
            "rfps_api_randomizer_domain": [1, 32],
            "phase1_fixture_text_len": PHASE1_FIXTURE_TEXT_LEN,
            "max_pattern_len": MAX_PATTERN_LEN,
            "phase1_fixture_transform_len": PHASE1_FIXTURE_TRANSFORM_LEN,
            "max_ckks_packed_slot_count": MAX_CKKS_BATCH_SIZE,
            "max_aligned_fps_score": 4_161_600,
            "max_aligned_rfps_score_magnitude": 522_240,
            "max_fps_prefix_square": 66_585_600,
            "dft_convention": "negative exponent; IDFT positive exponent and 1/N normalization",
            "dft_implementation": "rustfft FFT/IFFT; direct O(N^2) reference retained for equivalence tests",
            "ckks_data_type": "COMPLEX",
            "security_level": "HEStd_128_classic",
            "feer_reported": {
                "fps": {
                    "polynomial_modulus_degree": 32768,
                    "maximal_level": 1,
                    "coefficient_modulus_prime_bits": [60, 60],
                    "encoding_scale_bits": 40,
                    "decision_threshold": 0.5,
                    "response_rescaling": "one rescale after homomorphic computation"
                },
                "rfps": {
                    "polynomial_modulus_degree": 32768,
                    "maximal_level": 0,
                    "coefficient_modulus_prime_bits": [60],
                    "relinearization_special_prime_bits": 60,
                    "encoding_scale_bits": 20,
                    "decision_threshold": 4.0,
                    "randomizer_sampling_distribution": "uniform integers 3..=32",
                    "response_rescaling": "none"
                }
            },
            "openfhe_translation": {
                "parameter_mapping_note": "OpenFHE ScalingModSize controls its generated scaling prime and encoding scale, so this is an operational translation rather than a byte-for-byte reproduction of Feer's SEAL modulus chain",
                "fps": {
                    "scaling_technique": "FIXEDMANUAL",
                    "multiplicative_depth": 2,
                    "scaling_modulus_bits": 40,
                    "first_modulus_bits": 60,
                    "explicit_rescaling_operations": 1,
                    "scale_alignment_constant_multiplications": 1,
                    "decision_threshold": 0.5,
                    "empirical_raw_score_error_test_envelope": 1.0
                },
                "rfps": {
                    "scaling_technique": "FIXEDMANUAL",
                    "multiplicative_depth": 1,
                    "scaling_modulus_bits": RFPS_SCALING_MODULUS_BITS,
                    "initial_source_fidelity_scaling_modulus_bits": 20,
                    "parameter_change_reason": "20-bit mapping repeatedly rejected valid true matches at diagnostic threshold 0.5; 24 bits was the smallest tested setting with no rejection in the retained finite one-variable calibration",
                    "parameter_calibration_evidence": "docs/project/experiments/search/2026-09-06/p0c-openfhe-1.5.1-proposal-workloads/rfps_scaling_probe.json",
                    "first_modulus_bits": 60,
                    "explicit_rescaling_operations": 0,
                    "scale_alignment_constant_multiplications": 1,
                    "source_decision_threshold": 4.0,
                    "diagnostic_threshold": 0.5,
                    "empirical_raw_score_error_test_envelope": 2.0
                },
                "error_envelope_status": "observation-based test bounds through text/pattern lengths 1024/64; not theoretical or probabilistic guarantees",
                "bootstrapping_invoked": false
            },
            "diagnostic_vector_note": "fixed RFPS cancellation vectors include coordinates below 3 to reproduce Feer's explicit witness; benchmark randomizers use 3..=10"
        },
        "runs": {
            "fps": fps_json(&fps),
            "rfps": rfps_json(&rfps, 0.5)
        },
        "benchmarks": benchmarks,
        "synthetic_matrix": synthetic_matrix,
        "bgl_source": bgl_source,
        "bgl_cases": bgl_cases,
        "rfps_probability": rfps_probability,
        "domain_boundary": domain_boundary_case(),
        "serialization": {
            "fixed_vector": {
                "fps": serialization_json(
                    &fps_serialized.sizes,
                    fps_serialized.max_abs_error,
                    &fps_serialized.matching_positions
                ),
                "rfps": serialization_json(
                    &rfps_serialized.sizes,
                    rfps_serialized.max_abs_error,
                    &rfps_serialized.matching_positions
                )
            },
            "maximum_workload": maximum_serialization
        },
        "effect_separation": {
            "intrinsic_random_cancellation": {
                "observed": intrinsic.decisions[0].cause == DecisionCause::IntrinsicCancellation,
                "exact_score": intrinsic.exact_scores[0],
                "approximate_score": intrinsic.scores[0],
                "window_matches": false,
                "interpretation": "ordinary RFPS cancellation exists before CKKS approximation"
            },
            "threshold_only_acceptance": {
                "observed": threshold_stress.decisions[0].cause == DecisionCause::ThresholdOnly,
                "exact_score": threshold_stress.exact_scores[0],
                "approximate_score": threshold_stress.scores[0],
                "threshold": 4.0,
                "interpretation": "nonzero exact mismatch accepted by broad decision threshold"
            },
            "ckks_approximation": {
                "fps_max_abs_error": fps.max_abs_error,
                "rfps_max_abs_error": rfps.max_abs_error,
                "interpretation": "difference between decrypted/IDFT score and independent exact integer score"
            },
            "ckks_threshold_crossings": {
                "observed_count": ckks_crossings,
                "scope": "fixed diagnostics, legacy size probes, 28 matched matrix/BGL cases, 16,384 packed mismatch trials, and 1,024 packed true-match controls at threshold 0.5",
                "interpretation": "counted only when exact and approximate decisions differ; zero is a negative observation, not proof of impossibility"
            }
        },
        "failure_cases": [
            {
                "case": "negative decision threshold",
                "observed_rejection": invalid_threshold,
                "error": "InvalidDecisionThreshold"
            },
            {
                "case": "default FPS-14 smallest over-capacity text length 8192 gives L=16384",
                "observed_rejection": over_capacity,
                "error": "TransformExceedsProfileCapacity"
            },
            {
                "case": "non-power-of-two transform length 3",
                "observed_rejection": unsupported_transform,
                "error": "UnsupportedTransformLength(3)"
            },
            {
                "case": "RFPS threshold stress",
                "observed_failure": threshold_stress.matching_positions == vec![0],
                "effect": "false-positive acceptance caused by threshold, separate from exact random cancellation"
            },
            {
                "case": "initial RFPS ScalingModSize 20 at diagnostic threshold 0.5",
                "observed_failure": true,
                "effect": "fresh runs rejected valid fixture true matches; smallest tested observed-safe one-variable change was ScalingModSize 24",
                "raw_evidence": "docs/project/experiments/search/2026-09-06/p0c-openfhe-1.5.1-proposal-workloads/rfps_scale20_threshold_0.5_negative.txt",
                "calibration_evidence": "docs/project/experiments/search/2026-09-06/p0c-openfhe-1.5.1-proposal-workloads/rfps_scaling_probe.json",
                "uniform_guarantee": false
            }
        ],
        "memory": {
            "process_peak_rss_kib": process_peak_rss_kib(),
            "definition": "Linux VmHWM for experiment process"
        },
        "evidence_labels": {
            "construction_semantics": "literature-established",
            "layout_and_score_equivalence": "derived-and-validated",
            "encrypted_correctness_and_costs": "experimentally-observed",
            "privacy_and_security": "not established by this experiment"
        },
        "exact_command": "OPENFHE_INSTALL_DIR=$PWD/openfhe/.openfhe/openfhe-1.5.1/install LD_LIBRARY_PATH=$PWD/openfhe/.openfhe/openfhe-1.5.1/install/lib cargo run --release --locked -p cbpir-search --bin p0c_experiment --",
        "limitations": [
            "OpenFHE translation, not same-library reproduction of Feer's Microsoft SEAL implementation",
            "Agreement on tested vectors does not establish cryptographic privacy, security, or universal correctness",
            "OpenFHE exposes ciphertext state metadata here but no residual CKKS noise budget; metadata is not relabeled as noise budget",
            "GetLogPrecision is unavailable for COMPLEX CKKS in OpenFHE 1.5.1; precision is measured by independent decoded-score error",
            "Runtime samples are single process-local observations and are not cross-system comparisons",
            "The OpenFHE translation uses FIXEDMANUAL, one scale-alignment multiplication by constant 1 in each protocol, one explicit FPS rescale, no RFPS rescale, and no bootstrapping",
            "OpenFHE selected ring dimensions 16,384 for FPS and 8192 for RFPS; neither is Feer's default SEAL degree 32768",
            "OpenFHE ScalingModSize does not independently reproduce Feer's reported 60/60-bit FPS modulus chain with a 2^40 encoding scale through the wrapped high-level parameter API",
            "FPS configures two OpenFHE multiplication levels although the circuit uses one ciphertext-multiplication layer; the extra tower repaired the retained depth-one boundary failure, while insufficient depth-one modulus/scale headroom remains a candidate rather than a proved explanation",
            "An exploratory RFPS multiplicative-depth-0 translation was numerically invalid on the fixed vector, so retained OpenFHE multiplicative depth is 1 even though Feer describes RFPS maximal level L=0; these labels are not treated as interchangeable",
            "RFPS probability theory remains P0-B model-specific; this report imports its exact-count comparison target, independently scores the shared fixture, and separates intrinsic cancellation, threshold acceptance, and CKKS decision crossings",
            "RFPS ScalingModSize 24 is an empirical adjustment from Feer's reported 20-bit encoding scale after a retained 20-bit failure; finite calibration does not establish a uniform CKKS error bound"
        ]
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}
