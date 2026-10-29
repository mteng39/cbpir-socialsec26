#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionCause {
    TrueMatchAccepted,
    TrueMatchRejectedByCkks,
    MismatchRejected,
    IntrinsicCancellation,
    ThresholdOnly,
    CkksFalsePositiveThresholdCrossing,
    CkksFalseNegativeThresholdCrossing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreDecision {
    pub position: usize,
    pub exact_score: i64,
    pub approximate_score: f64,
    pub window_matches: bool,
    pub cause: DecisionCause,
}

#[must_use]
#[allow(clippy::cast_precision_loss)] // P0-C's declared exact-score bound is below 2^53.
pub fn classify_score(
    window_matches: bool,
    exact_score: i64,
    approximate_score: f64,
    threshold: f64,
) -> DecisionCause {
    let exact_accepted = exact_score.unsigned_abs() as f64 <= threshold;
    let approximate_accepted = approximate_score.abs() <= threshold;

    if window_matches {
        return if approximate_accepted {
            DecisionCause::TrueMatchAccepted
        } else {
            DecisionCause::TrueMatchRejectedByCkks
        };
    }
    if exact_score == 0 && approximate_accepted {
        return DecisionCause::IntrinsicCancellation;
    }
    match (exact_accepted, approximate_accepted) {
        (true, true) => DecisionCause::ThresholdOnly,
        (true, false) => DecisionCause::CkksFalseNegativeThresholdCrossing,
        (false, true) => DecisionCause::CkksFalsePositiveThresholdCrossing,
        (false, false) => DecisionCause::MismatchRejected,
    }
}
