//! Explicit named-profile Match adapter for the Phase-2 composition runner.

use crate::{Catalog, MatchMode, MatchRequest, P0dError};
use cbpir_search::{
    DecisionCause, FpsProfile, RfpsProfile, ServerTextCtPtOperationCounts, ServerTextCtPtTimings,
    server_text_ct_pt_fps_with_profile, server_text_ct_pt_rfps_with_profile,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug)]
pub struct Phase2Profile {
    pub ring_dimension: usize,
    pub fps: FpsProfile,
    pub rfps: RfpsProfile,
}

impl Phase2Profile {
    pub fn for_ring_dimension(ring_dimension: usize) -> Result<Self, P0dError> {
        match ring_dimension {
            8192 => Ok(Self {
                ring_dimension,
                fps: FpsProfile::Fps13,
                rfps: RfpsProfile::Rfps13,
            }),
            16384 => Ok(Self {
                ring_dimension,
                fps: FpsProfile::Fps14,
                rfps: RfpsProfile::Rfps14,
            }),
            32768 => Ok(Self {
                ring_dimension,
                fps: FpsProfile::Fps15,
                rfps: RfpsProfile::Rfps15,
            }),
            _ => Err(P0dError::Request(
                "Phase-2 ring dimension must be 8192, 16384, or 32768",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Phase2DecisionMismatch {
    pub position: usize,
    pub threshold: f64,
    pub exact_score: f64,
    pub decrypted_score: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Phase2MatchTrace {
    pub backend: &'static str,
    pub mode: &'static str,
    pub profile: &'static str,
    pub security: &'static str,
    pub ring_dimension: u32,
    pub batch_size: u32,
    pub multiplicative_depth: u32,
    pub first_modulus_bits: u32,
    pub scaling_modulus_bits: u32,
    pub scaling_technique: String,
    pub key_switch_technique: String,
    pub scores: Vec<f64>,
    pub exact_scores: Vec<f64>,
    pub matching_positions: Vec<usize>,
    pub exact_matching_positions: Vec<usize>,
    pub max_abs_error: f64,
    pub decision_mismatches: usize,
    pub decision_mismatch_details: Vec<Phase2DecisionMismatch>,
    pub intrinsic_cancellation_count: usize,
    pub threshold_only_count: usize,
    pub true_match_accepted_count: usize,
    pub operations: ServerTextCtPtOperationCounts,
    pub timings: ServerTextCtPtTimings,
}

fn mismatch_details(
    scores: &[f64],
    exact_scores: &[f64],
    threshold: f64,
) -> Vec<Phase2DecisionMismatch> {
    scores
        .iter()
        .copied()
        .zip(exact_scores.iter().copied())
        .enumerate()
        .filter_map(|(position, (decrypted_score, exact_score))| {
            ((decrypted_score.abs() <= threshold) != (exact_score.abs() <= threshold)).then_some(
                Phase2DecisionMismatch {
                    position,
                    threshold,
                    exact_score,
                    decrypted_score,
                },
            )
        })
        .collect()
}

pub fn run_phase2_match(
    catalog: &Catalog,
    request: &MatchRequest,
    profile: Phase2Profile,
) -> Result<Phase2MatchTrace, P0dError> {
    catalog.validate(request)?;
    match &request.mode {
        MatchMode::Fps => {
            let result = server_text_ct_pt_fps_with_profile(
                catalog.text(),
                &request.pattern,
                request.threshold,
                profile.fps,
            )
            .map_err(|error| P0dError::MatchComponent(format!("FPS: {error:?}")))?;
            let exact_matching_positions = result
                .exact_scores
                .iter()
                .enumerate()
                .filter_map(|(position, score)| {
                    (score.abs() <= request.threshold).then_some(position)
                })
                .collect::<Vec<_>>();
            let details = mismatch_details(&result.scores, &result.exact_scores, request.threshold);
            Ok(Phase2MatchTrace {
                backend: "server_text_ct_pt",
                mode: "fps",
                profile: profile.fps.label(),
                security: "HEStd_128_classic",
                ring_dimension: result.metrics.ring_dimension,
                batch_size: result.metrics.batch_size,
                multiplicative_depth: result.metrics.configured_multiplicative_depth,
                first_modulus_bits: result.metrics.first_modulus_bits,
                scaling_modulus_bits: result.metrics.scaling_modulus_bits,
                scaling_technique: result.metrics.scaling_technique,
                key_switch_technique: result.metrics.key_switch_technique,
                scores: result.scores,
                exact_scores: result.exact_scores,
                matching_positions: result.matching_positions,
                exact_matching_positions,
                max_abs_error: result.max_abs_error,
                decision_mismatches: details.len(),
                decision_mismatch_details: details,
                intrinsic_cancellation_count: 0,
                threshold_only_count: 0,
                true_match_accepted_count: 0,
                operations: result.metrics.operations,
                timings: result.metrics.timings,
            })
        }
        MatchMode::Rfps { randomizer } => {
            let result = server_text_ct_pt_rfps_with_profile(
                catalog.text(),
                &request.pattern,
                randomizer,
                request.threshold,
                profile.rfps,
            )
            .map_err(|error| P0dError::MatchComponent(format!("RFPS: {error:?}")))?;
            let exact_scores = result
                .exact_scores
                .iter()
                .map(|&score| score as f64)
                .collect::<Vec<_>>();
            let exact_matching_positions = exact_scores
                .iter()
                .enumerate()
                .filter_map(|(position, score)| {
                    (score.abs() <= request.threshold).then_some(position)
                })
                .collect::<Vec<_>>();
            let details = mismatch_details(&result.scores, &exact_scores, request.threshold);
            let count = |cause| {
                result
                    .decisions
                    .iter()
                    .filter(|decision| decision.cause == cause)
                    .count()
            };
            Ok(Phase2MatchTrace {
                backend: "server_text_ct_pt",
                mode: "rfps",
                profile: profile.rfps.label(),
                security: "HEStd_128_classic",
                ring_dimension: result.metrics.ring_dimension,
                batch_size: result.metrics.batch_size,
                multiplicative_depth: result.metrics.configured_multiplicative_depth,
                first_modulus_bits: result.metrics.first_modulus_bits,
                scaling_modulus_bits: result.metrics.scaling_modulus_bits,
                scaling_technique: result.metrics.scaling_technique,
                key_switch_technique: result.metrics.key_switch_technique,
                scores: result.scores,
                exact_scores,
                matching_positions: result.matching_positions,
                exact_matching_positions,
                max_abs_error: result.max_abs_error,
                decision_mismatches: details.len(),
                decision_mismatch_details: details,
                intrinsic_cancellation_count: count(DecisionCause::IntrinsicCancellation),
                threshold_only_count: count(DecisionCause::ThresholdOnly),
                true_match_accepted_count: count(DecisionCause::TrueMatchAccepted),
                operations: result.metrics.operations,
                timings: result.metrics.timings,
            })
        }
    }
}
