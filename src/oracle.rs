use crate::{Catalog, MatchMode, MatchRequest, P0dError, ReconstructedRecord, RecordId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct OracleWindow {
    pub id: RecordId,
    pub local_offset: usize,
    pub global_position: usize,
    pub exact_score: i32,
    pub semantic_match: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct OracleEvaluation {
    pub windows: Vec<OracleWindow>,
    pub semantic_ids: Vec<RecordId>,
    pub exact_ids: Vec<RecordId>,
    pub semantic_record: Option<ReconstructedRecord>,
    pub exact_record: Option<ReconstructedRecord>,
}

pub fn oracle_records(
    catalog: &Catalog,
    ids: &[RecordId],
    limit: usize,
) -> Result<Vec<ReconstructedRecord>, P0dError> {
    ids.iter()
        .take(limit)
        .map(|id| {
            catalog
                .records()
                .iter()
                .find(|record| record.id == *id)
                .map(|record| ReconstructedRecord {
                    id: record.id,
                    payload: record.payload.clone(),
                })
                .ok_or(P0dError::Target("oracle ID outside catalog"))
        })
        .collect()
}
#[derive(Clone, Debug, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent evidence flags can overlap; they are not state-machine states"
)]
pub struct DecisionComparison {
    pub global_position: usize,
    pub id: RecordId,
    pub exact_score: i32,
    pub approximate_score: f64,
    pub semantic_match: bool,
    pub exact_accepted: bool,
    pub approximate_accepted: bool,
    pub intrinsic_cancellation: bool,
    pub threshold_only: bool,
    pub ckks_false_positive: bool,
    pub ckks_false_negative: bool,
}

/// Direct logical-record oracle; it never uses component scoring or Locate.
pub fn oracle(c: &Catalog, q: &MatchRequest) -> Result<OracleEvaluation, P0dError> {
    c.validate(q)?;
    let mut windows = Vec::new();
    let mut semantic_ids = Vec::new();
    let mut exact_ids = Vec::new();
    let mut global = 0;
    for r in c.records() {
        for (offset, w) in r.searchable.windows(q.pattern.len()).enumerate() {
            // Maximum score is 64*255^2, which fits i32 exactly.
            let score = w
                .iter()
                .zip(&q.pattern)
                .enumerate()
                .map(|(j, (&a, &b))| {
                    let d = i32::from(a) - i32::from(b);
                    match &q.mode {
                        MatchMode::Fps => d * d,
                        MatchMode::Rfps { randomizer } => d * i32::from(randomizer[j]),
                    }
                })
                .sum::<i32>();
            let same = w == q.pattern;
            if same && semantic_ids.last() != Some(&r.id) {
                semantic_ids.push(r.id);
            }
            if f64::from(score).abs() <= q.threshold && exact_ids.last() != Some(&r.id) {
                exact_ids.push(r.id);
            }
            windows.push(OracleWindow {
                id: r.id,
                local_offset: offset,
                global_position: global + offset,
                exact_score: score,
                semantic_match: same,
            });
        }
        global += r.searchable.len() + 1;
    }
    let fetch = |ids: &[RecordId]| {
        ids.first()
            .and_then(|id| c.records().iter().find(|r| r.id == *id))
            .map(|r| ReconstructedRecord {
                id: r.id,
                payload: r.payload.clone(),
            })
    };
    Ok(OracleEvaluation {
        semantic_record: fetch(&semantic_ids),
        exact_record: fetch(&exact_ids),
        windows,
        semantic_ids,
        exact_ids,
    })
}
pub fn compare(
    o: &OracleEvaluation,
    scores: &[f64],
    threshold: f64,
) -> Result<Vec<DecisionComparison>, P0dError> {
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(P0dError::MatchContract("invalid comparison threshold"));
    }
    o.windows
        .iter()
        .map(|w| {
            let x = *scores
                .get(w.global_position)
                .ok_or(P0dError::MatchContract("missing oracle-aligned score"))?;
            if !x.is_finite() {
                return Err(P0dError::MatchContract("nonfinite comparison score"));
            }
            let exact = f64::from(w.exact_score).abs() <= threshold;
            let approx = x.abs() <= threshold;
            Ok(DecisionComparison {
                global_position: w.global_position,
                id: w.id,
                exact_score: w.exact_score,
                approximate_score: x,
                semantic_match: w.semantic_match,
                exact_accepted: exact,
                approximate_accepted: approx,
                intrinsic_cancellation: !w.semantic_match && w.exact_score == 0,
                threshold_only: !w.semantic_match && w.exact_score != 0 && exact,
                ckks_false_positive: !exact && approx,
                ckks_false_negative: exact && !approx,
            })
        })
        .collect()
}
