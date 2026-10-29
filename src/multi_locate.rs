use crate::{Candidate, Catalog, MatchRequest, P0dError, locate, locate_validated};
use serde::Serialize;

pub const MULTI_PUBLIC_K: [usize; 4] = [1, 4, 8, 16];

#[derive(Clone, Debug, Serialize)]
pub struct MultiLocateTrace {
    pub public_k: usize,
    pub accepted_positions: Vec<usize>,
    pub boundary_rejected_positions: Vec<usize>,
    pub candidates: Vec<Candidate>,
    pub selected_candidates: Vec<Candidate>,
    pub duplicate_count: usize,
    pub truncated_count: usize,
}

pub fn validate_multi_public_k(public_k: usize) -> Result<(), P0dError> {
    if MULTI_PUBLIC_K.contains(&public_k) {
        Ok(())
    } else {
        Err(P0dError::Request("public K must be 1, 4, 8, or 16"))
    }
}

pub fn locate_many(
    catalog: &Catalog,
    request: &MatchRequest,
    scores: &[f64],
    positions: &[usize],
    public_k: usize,
) -> Result<MultiLocateTrace, P0dError> {
    validate_multi_public_k(public_k)?;
    let one = locate(catalog, request, scores, positions)?;
    let mut candidates = one.candidates;
    candidates.sort_by_key(|candidate| (candidate.id, candidate.global_position));
    let selected_candidates = candidates.iter().take(public_k).cloned().collect();
    let truncated_count = candidates.len().saturating_sub(public_k);

    Ok(MultiLocateTrace {
        public_k,
        accepted_positions: one.accepted_positions,
        boundary_rejected_positions: one.boundary_rejected_positions,
        candidates,
        selected_candidates,
        duplicate_count: one.duplicate_count,
        truncated_count,
    })
}

/// Performs one threshold pass followed by boundary filtering, stable-ID
/// deduplication, sorting, and deterministic `First_K` selection.
pub fn locate_many_from_scores(
    catalog: &Catalog,
    request: &MatchRequest,
    scores: &[f64],
    public_k: usize,
) -> Result<MultiLocateTrace, P0dError> {
    validate_multi_public_k(public_k)?;
    catalog.validate(request)?;
    let expected = catalog.text().len() - request.pattern.len() + 1;
    if scores.len() != expected {
        return Err(P0dError::MatchContract("wrong score count"));
    }
    let mut positions = Vec::new();
    for (position, score) in scores.iter().copied().enumerate() {
        if !score.is_finite() {
            return Err(P0dError::MatchContract("nonfinite score"));
        }
        if score.abs() <= request.threshold {
            positions.push(position);
        }
    }
    let one = locate_validated(catalog, request, scores, &positions)?;
    let mut candidates = one.candidates;
    candidates.sort_by_key(|candidate| (candidate.id, candidate.global_position));
    let selected_candidates = candidates.iter().take(public_k).cloned().collect();
    let truncated_count = candidates.len().saturating_sub(public_k);
    Ok(MultiLocateTrace {
        public_k,
        accepted_positions: one.accepted_positions,
        boundary_rejected_positions: one.boundary_rejected_positions,
        candidates,
        selected_candidates,
        duplicate_count: one.duplicate_count,
        truncated_count,
    })
}
