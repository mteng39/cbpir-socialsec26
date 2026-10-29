use crate::{Catalog, FetchTarget, MatchRequest, P0dError, RecordId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub id: RecordId,
    pub physical_index: usize,
    pub global_position: usize,
    pub local_offset: usize,
    pub score: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct LocateTrace {
    pub accepted_positions: Vec<usize>,
    pub boundary_rejected_positions: Vec<usize>,
    pub candidates: Vec<Candidate>,
    pub duplicate_count: usize,
    pub truncated_count: usize,
    pub target: FetchTarget,
}
pub fn validate_scores(
    scores: &[f64],
    positions: &[usize],
    expected: usize,
    threshold: f64,
) -> Result<(), P0dError> {
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(P0dError::MatchContract("invalid threshold"));
    }
    if scores.len() != expected {
        return Err(P0dError::MatchContract("wrong score count"));
    }
    if scores.iter().any(|s| !s.is_finite()) {
        return Err(P0dError::MatchContract("nonfinite score"));
    }
    if positions.iter().any(|p| *p >= expected) {
        return Err(P0dError::MatchContract("out-of-range position"));
    }
    if positions.windows(2).any(|w| w[0] >= w[1]) {
        return Err(P0dError::MatchContract("positions not strictly ascending"));
    }
    let recomputed: Vec<_> = scores
        .iter()
        .enumerate()
        .filter_map(|(p, s)| (s.abs() <= threshold).then_some(p))
        .collect();
    if recomputed != positions {
        return Err(P0dError::MatchContract(
            "positions inconsistent with scores",
        ));
    }
    Ok(())
}
pub fn locate(
    c: &Catalog,
    q: &MatchRequest,
    scores: &[f64],
    positions: &[usize],
) -> Result<LocateTrace, P0dError> {
    c.validate(q)?;
    validate_scores(
        scores,
        positions,
        c.text().len() - q.pattern.len() + 1,
        q.threshold,
    )?;
    locate_validated(c, q, scores, positions)
}

pub(crate) fn locate_validated(
    c: &Catalog,
    q: &MatchRequest,
    scores: &[f64],
    positions: &[usize],
) -> Result<LocateTrace, P0dError> {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut rejected = Vec::new();
    let mut duplicates = 0;
    for &p in positions {
        let end = p
            .checked_add(q.pattern.len())
            .ok_or(P0dError::OffsetOverflow)?;
        let Some(span) = c.spans().iter().find(|s| s.start <= p && end <= s.end) else {
            rejected.push(p);
            continue;
        };
        if candidates.last().is_some_and(|last| last.id == span.id) {
            duplicates += 1;
            continue;
        }
        candidates.push(Candidate {
            id: span.id,
            physical_index: span.physical_index,
            global_position: p,
            local_offset: p - span.start,
            score: scores[p],
        });
    }
    let target = candidates.first().map_or(
        FetchTarget::Dummy {
            physical_index: c.dummy_index(),
        },
        |x| FetchTarget::Real {
            id: x.id,
            physical_index: x.physical_index,
        },
    );
    Ok(LocateTrace {
        accepted_positions: positions.to_vec(),
        boundary_rejected_positions: rejected,
        duplicate_count: duplicates,
        truncated_count: candidates.len().saturating_sub(1),
        candidates,
        target,
    })
}
