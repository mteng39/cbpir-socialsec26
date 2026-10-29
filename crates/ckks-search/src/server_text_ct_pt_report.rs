//! Focused matched report for the additive server-owned-text CKKS variant.

use std::time::Instant;

use serde_json::{Value, json};

use crate::{
    CkksError, CkksRunMetrics, DecisionCause, RfpsPackedTrialInput, ServerTextCtPtArtifactSizes,
    ServerTextCtPtMetrics, ckks_fps, ckks_fps_serialization_roundtrip, ckks_rfps,
    ckks_rfps_packed_trials, ckks_rfps_serialization_roundtrip, server_text_ct_pt_fps,
    server_text_ct_pt_fps_artifact_sizes, server_text_ct_pt_rfps,
    server_text_ct_pt_rfps_artifact_sizes, server_text_ct_pt_rfps_packed_artifact_sizes,
    server_text_ct_pt_rfps_packed_trials,
};

fn duration_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn baseline_metrics(metrics: &CkksRunMetrics) -> Value {
    let phase_timings = metrics.phase_timings.map(|timings| {
        json!({
            "context_and_key_generation_ns": timings.context_and_key_generation_ns,
            "client_setup_ns": timings.client_setup_ns,
            "client_query_ns": timings.client_query_ns,
            "server_evaluation_ns": timings.server_evaluation_ns,
            "client_decrypt_idft_ns": timings.client_decrypt_idft_ns,
            "client_decrypt_idft_note": "includes exact-oracle/error/decision instrumentation",
        })
    });
    json!({
        "backend": "ct_ct",
        "openfhe_version": metrics.openfhe_version,
        "ring_dimension": metrics.ring_dimension,
        "batch_size": metrics.batch_size,
        "configured_multiplicative_depth": metrics.multiplicative_depth,
        "scaling_modulus_bits": metrics.scaling_modulus_bits,
        "first_modulus_bits": metrics.first_modulus_bits,
        "phase_timings": phase_timings,
        "operations": {
            "ciphertext_multiplications": metrics.ciphertext_multiplications,
            "ciphertext_plaintext_multiplications": metrics.ciphertext_plaintext_multiplications,
            "ciphertext_constant_multiplications": metrics.ciphertext_constant_multiplications,
            "additions": metrics.additions,
            "subtractions": metrics.subtractions,
            "relinearizations": metrics.relinearizations,
            "explicit_rescaling_operations": metrics.explicit_rescaling_operations,
        },
        "required_keys": {
            "public_key": true,
            "secret_key": true,
            "evaluation_multiplication_key": true,
            "relinearization_key": true,
            "rotation_keys": false,
        },
    })
}

fn variant_metrics(metrics: &ServerTextCtPtMetrics) -> Value {
    json!({
        "backend": metrics.backend,
        "openfhe_version": metrics.openfhe_version,
        "ring_dimension": metrics.ring_dimension,
        "batch_size": metrics.batch_size,
        "configured_multiplicative_depth": metrics.configured_multiplicative_depth,
        "scaling_modulus_bits": metrics.scaling_modulus_bits,
        "first_modulus_bits": metrics.first_modulus_bits,
        "phase_timings": {
            "context_generation_ns": metrics.timings.context_generation_ns,
            "key_generation_ns": metrics.timings.key_generation_ns,
            "server_text_dft_preparation_ns": metrics.timings.server_text_dft_preparation_ns,
            "server_plaintext_encoding_ns": metrics.timings.server_plaintext_encoding_ns,
            "server_query_plaintext_preparation_ns": metrics.timings.server_query_plaintext_preparation_ns,
            "client_query_dft_preparation_ns": metrics.timings.client_query_dft_preparation_ns,
            "client_query_encryption_ns": metrics.timings.client_query_encryption_ns,
            "server_homomorphic_evaluation_ns": metrics.timings.server_homomorphic_evaluation_ns,
            "client_decrypt_ns": metrics.timings.client_decrypt_ns,
            "client_idft_ns": metrics.timings.client_idft_ns,
            "oracle_error_instrumentation_ns": metrics.timings.oracle_error_instrumentation_ns,
            "derived_online_match_ns": metrics.timings.derived_online_match_ns(),
            "cold_end_to_end_ns": metrics.timings.cold_end_to_end_ns(),
        },
        "operations": metrics.operations,
        "required_keys": metrics.keys,
        "state_sizes": metrics.state_sizes,
    })
}

fn variant_sizes(sizes: &ServerTextCtPtArtifactSizes) -> Value {
    json!(sizes)
}

fn trial(id: &str, text: &[u8], pattern: &[u8], randomizer: &[u8]) -> RfpsPackedTrialInput {
    RfpsPackedTrialInput {
        id: id.to_owned(),
        text: text.to_vec(),
        pattern: pattern.to_vec(),
        randomizer: randomizer.to_vec(),
    }
}

/// Runs a bounded matched ct×ct versus server-text ct×pt P0-C validation.
///
/// Repetitions use identical parameters and inputs. Each repetition runs the
/// frozen `ct_ct` backend first and the additive `server_text_ct_pt` backend
/// second. Raw timings are retained; unlike phase boundaries are not divided
/// into a synthetic speedup.
///
/// # Errors
///
/// Returns [`CkksError`] if any retained P0-C operation or size probe fails.
#[allow(clippy::too_many_lines)]
#[allow(
    clippy::similar_names,
    reason = "paired FPS/RFPS baseline and variant labels are deliberate report dimensions"
)]
pub fn server_text_ct_pt_focused_report(repetitions: usize) -> Result<Value, CkksError> {
    if repetitions == 0 {
        return Err(CkksError::EmptyPackedTrials);
    }

    let fps_text = [1, 2, 3, 4, 1, 2, 3];
    let fps_pattern = [2, 3, 4];
    let baseline_fps = ckks_fps(&fps_text, &fps_pattern, 0.5)?;
    let variant_fps = server_text_ct_pt_fps(&fps_text, &fps_pattern, 0.5)?;

    let exact_text = [1, 2];
    let exact_pattern = [1, 2];
    let exact_randomizer = [3, 4];
    let baseline_rfps = ckks_rfps(&exact_text, &exact_pattern, &exact_randomizer, 0.5)?;
    let variant_rfps = server_text_ct_pt_rfps(&exact_text, &exact_pattern, &exact_randomizer, 0.5)?;

    let cancellation = server_text_ct_pt_rfps(&[2, 0], &[1, 1], &[1, 1], 0.5)?;
    let threshold_only = server_text_ct_pt_rfps(&[2], &[1], &[3], 4.0)?;

    let packed_inputs = vec![
        trial("left", &[0, 2, 1], &[2, 0, 1], &[3, 5, 7]),
        trial("match", &[2, 1, 0], &[2, 1, 0], &[11, 13, 17]),
        trial("right", &[2, 0, 1], &[0, 1, 2], &[19, 23, 29]),
    ];
    let baseline_packed = ckks_rfps_packed_trials(&packed_inputs, 0.5)?;
    let variant_packed = server_text_ct_pt_rfps_packed_trials(&packed_inputs, 0.5)?;
    let packed_trial_isolation = baseline_packed
        .trials
        .iter()
        .zip(&variant_packed.trials)
        .all(|(baseline, variant)| {
            baseline.id == variant.id && baseline.exact_score == variant.exact_score
        });

    let mut matched_runs = Vec::with_capacity(repetitions);
    for repetition in 0..repetitions {
        let started = Instant::now();
        let baseline_fps_run = ckks_fps(&fps_text, &fps_pattern, 0.5)?;
        let baseline_fps_outer_ns = duration_ns(started);
        let started = Instant::now();
        let variant_fps_run = server_text_ct_pt_fps(&fps_text, &fps_pattern, 0.5)?;
        let variant_fps_outer_ns = duration_ns(started);
        let started = Instant::now();
        let baseline_rfps_run = ckks_rfps(&exact_text, &exact_pattern, &exact_randomizer, 0.5)?;
        let baseline_rfps_outer_ns = duration_ns(started);
        let started = Instant::now();
        let variant_rfps_run =
            server_text_ct_pt_rfps(&exact_text, &exact_pattern, &exact_randomizer, 0.5)?;
        let variant_rfps_outer_ns = duration_ns(started);
        matched_runs.push(json!({
            "repetition": repetition,
            "ordering": ["fps_ct_ct", "fps_server_text_ct_pt", "rfps_ct_ct", "rfps_server_text_ct_pt"],
            "fps": {
                "ct_ct": {"outer_ns": baseline_fps_outer_ns, "metrics": baseline_metrics(&baseline_fps_run.metrics)},
                "server_text_ct_pt": {"outer_ns": variant_fps_outer_ns, "metrics": variant_metrics(&variant_fps_run.metrics)},
            },
            "rfps": {
                "ct_ct": {"outer_ns": baseline_rfps_outer_ns, "metrics": baseline_metrics(&baseline_rfps_run.metrics)},
                "server_text_ct_pt": {"outer_ns": variant_rfps_outer_ns, "metrics": variant_metrics(&variant_rfps_run.metrics)},
            },
        }));
    }

    let baseline_fps_sizes = ckks_fps_serialization_roundtrip(&fps_text, &fps_pattern, 0.5)?;
    let baseline_rfps_sizes =
        ckks_rfps_serialization_roundtrip(&exact_text, &exact_pattern, &exact_randomizer, 0.5)?;
    let variant_fps_sizes = server_text_ct_pt_fps_artifact_sizes(&fps_text, &fps_pattern, 0.5)?;
    let variant_rfps_sizes =
        server_text_ct_pt_rfps_artifact_sizes(&exact_text, &exact_pattern, &exact_randomizer, 0.5)?;
    let variant_packed_sizes = server_text_ct_pt_rfps_packed_artifact_sizes(&packed_inputs, 0.5)?;

    let fps_positions_agree = baseline_fps.matching_positions == variant_fps.matching_positions;
    let rfps_positions_agree = baseline_rfps.matching_positions == variant_rfps.matching_positions;
    let cancellation_classification = cancellation.decisions[0].cause;
    let threshold_classification = threshold_only.decisions[0].cause;
    let gate_passed = fps_positions_agree
        && rfps_positions_agree
        && variant_fps.max_abs_error <= 1e-3
        && variant_rfps.max_abs_error <= 0.02
        && cancellation_classification == DecisionCause::IntrinsicCancellation
        && threshold_classification == DecisionCause::ThresholdOnly
        && packed_trial_isolation
        && variant_packed.metrics.operations.ciphertext_multiplications == 0;

    Ok(json!({
        "schema": "p0c-server-text-ct-pt-v1",
        "classification": "bounded local focused validation",
        "baseline_backend": "ct_ct",
        "variant_backend": "server_text_ct_pt",
        "gate_passed": gate_passed,
        "repetitions": repetitions,
        "validation": {
            "fps": {
                "positions_agree": fps_positions_agree,
                "ct_ct_positions": baseline_fps.matching_positions,
                "server_text_ct_pt_positions": variant_fps.matching_positions,
                "exact_scores": variant_fps.exact_scores,
                "ct_ct_scores": baseline_fps.scores,
                "server_text_ct_pt_scores": variant_fps.scores,
                "server_text_ct_pt_max_abs_error": variant_fps.max_abs_error,
            },
            "rfps": {
                "positions_agree": rfps_positions_agree,
                "ct_ct_positions": baseline_rfps.matching_positions,
                "server_text_ct_pt_positions": variant_rfps.matching_positions,
                "exact_scores": variant_rfps.exact_scores,
                "ct_ct_scores": baseline_rfps.scores,
                "server_text_ct_pt_scores": variant_rfps.scores,
                "server_text_ct_pt_max_abs_error": variant_rfps.max_abs_error,
            },
            "rfps_cancellation": {
                "exact_score": cancellation.exact_scores[0],
                "approximate_score": cancellation.scores[0],
                "threshold": 0.5,
                "classification": format!("{cancellation_classification:?}"),
            },
            "rfps_threshold_only": {
                "exact_score": threshold_only.exact_scores[0],
                "approximate_score": threshold_only.scores[0],
                "threshold": 4.0,
                "classification": format!("{threshold_classification:?}"),
            },
            "packed_rfps": {
                "trial_isolation": packed_trial_isolation,
                "block_transform_len": variant_packed.block_transform_len,
                "encoded_slot_count": variant_packed.encoded_slot_count,
                "padding_slot_count": variant_packed.padding_slot_count,
                "trials": variant_packed.trials.iter().map(|trial| json!({
                    "id": trial.id,
                    "exact_score": trial.exact_score,
                    "approximate_score": trial.approximate_score,
                    "classification": format!("{:?}", trial.decision.cause),
                })).collect::<Vec<_>>(),
            },
        },
        "operation_audit": {
            "ct_ct": {
                "fps": baseline_metrics(&baseline_fps.metrics)["operations"],
                "rfps": baseline_metrics(&baseline_rfps.metrics)["operations"],
                "packed_rfps": baseline_metrics(&baseline_packed.metrics)["operations"],
            },
            "server_text_ct_pt": {
                "fps": variant_metrics(&variant_fps.metrics)["operations"],
                "rfps": variant_metrics(&variant_rfps.metrics)["operations"],
                "packed_rfps": variant_metrics(&variant_packed.metrics)["operations"],
            },
        },
        "key_audit": {
            "ct_ct": baseline_metrics(&baseline_fps.metrics)["required_keys"],
            "server_text_ct_pt": variant_metrics(&variant_fps.metrics)["required_keys"],
            "runtime_evidence": "server_text_ct_pt evaluation completed after clearing global eval-multiplication keys and without EvalMultKeyGen",
        },
        "artifact_sizes": {
            "ct_ct": {
                "fps": {
                    "context_bytes": baseline_fps_sizes.sizes.context_bytes,
                    "public_key_bytes": baseline_fps_sizes.sizes.public_key_bytes,
                    "secret_key_bytes": baseline_fps_sizes.sizes.private_key_bytes,
                    "evaluation_multiplication_key_bytes": baseline_fps_sizes.sizes.eval_mult_key_bytes,
                    "setup_ciphertext_bytes": baseline_fps_sizes.sizes.setup_ciphertext_bytes,
                    "encrypted_query_ciphertext_bytes": baseline_fps_sizes.sizes.query_ciphertext_bytes,
                    "encrypted_response_ciphertext_bytes": baseline_fps_sizes.sizes.response_ciphertext_bytes,
                },
                "rfps": {
                    "context_bytes": baseline_rfps_sizes.sizes.context_bytes,
                    "public_key_bytes": baseline_rfps_sizes.sizes.public_key_bytes,
                    "secret_key_bytes": baseline_rfps_sizes.sizes.private_key_bytes,
                    "evaluation_multiplication_key_bytes": baseline_rfps_sizes.sizes.eval_mult_key_bytes,
                    "setup_ciphertext_bytes": baseline_rfps_sizes.sizes.setup_ciphertext_bytes,
                    "encrypted_query_ciphertext_bytes": baseline_rfps_sizes.sizes.query_ciphertext_bytes,
                    "encrypted_response_ciphertext_bytes": baseline_rfps_sizes.sizes.response_ciphertext_bytes,
                },
                "packed_rfps": null,
            },
            "server_text_ct_pt": {
                "fps": variant_sizes(&variant_fps_sizes),
                "rfps": variant_sizes(&variant_rfps_sizes),
                "packed_rfps": variant_sizes(&variant_packed_sizes),
            },
            "encoded_plaintext_size_note": "pinned OpenFHE-rs exposes no plaintext-object serializer; numerical DFT bytes and encoded object count are reported",
        },
        "matched_timing_runs": matched_runs,
        "timing_comparison_rule": "Only identical outer function boundaries are directly comparable. Fine phase timings are retained raw because baseline and variant instrumentation boundaries differ.",
        "decision_differences": {
            "fps_positions": if fps_positions_agree { Vec::<usize>::new() } else { variant_fps.matching_positions.clone() },
            "rfps_positions": if rfps_positions_agree { Vec::<usize>::new() } else { variant_rfps.matching_positions.clone() },
            "note": "Approximate scores may differ; exact scores and threshold classifications remain separately retained.",
        },
        "limitations": [
            "single-process logical role separation; no deployed network isolation",
            "bounded fixtures are not a deployment probability or scaling claim",
            "server owns plaintext text; its numerical DFT remains server-side",
            "Feer protocol theorem is not claimed verbatim for this changed text-ownership setup",
        ],
    }))
}
