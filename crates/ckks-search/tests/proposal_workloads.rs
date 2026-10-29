use cbpir_search::{DecisionCause, ckks_fps, ckks_rfps, classify_score, proposal_workloads};

fn accepted(cause: DecisionCause) -> bool {
    matches!(
        cause,
        DecisionCause::TrueMatchAccepted
            | DecisionCause::IntrinsicCancellation
            | DecisionCause::ThresholdOnly
            | DecisionCause::CkksFalsePositiveThresholdCrossing
    )
}

fn validate_case(
    id: &str,
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    expected_positions: &[usize],
) {
    let fps = ckks_fps(text, pattern, 0.5).unwrap_or_else(|error| panic!("{id} FPS: {error:?}"));
    let rfps = ckks_rfps(text, pattern, randomizer, 0.5)
        .unwrap_or_else(|error| panic!("{id} RFPS: {error:?}"));

    let exact_fps_positions = fps
        .exact_scores
        .iter()
        .enumerate()
        .filter_map(|(position, score)| (score.abs() <= f64::EPSILON).then_some(position))
        .collect::<Vec<_>>();
    assert_eq!(exact_fps_positions, expected_positions, "{id} exact FPS");

    for &position in expected_positions {
        assert!(
            fps.matching_positions.contains(&position),
            "{id} FPS lost true match {position}; score={}",
            fps.scores[position]
        );
        assert!(
            rfps.matching_positions.contains(&position),
            "{id} RFPS lost true match {position}; score={}",
            rfps.scores[position]
        );
    }

    for threshold in [0.5, 4.0] {
        for (position, (&exact_score, &approximate_score)) in
            rfps.exact_scores.iter().zip(&rfps.scores).enumerate()
        {
            let window_matches = text[position..position + pattern.len()] == *pattern;
            let cause = classify_score(window_matches, exact_score, approximate_score, threshold);
            assert_eq!(
                accepted(cause),
                approximate_score.abs() <= threshold,
                "{id} position {position} threshold {threshold}"
            );
            if window_matches {
                assert_eq!(
                    cause,
                    DecisionCause::TrueMatchAccepted,
                    "{id} position {position}"
                );
            }
        }
    }
}

#[test]
fn proposal_matrix_and_bgl_preserve_true_matches_and_classify_all_rfps_scores() {
    let fixture = proposal_workloads().expect("canonical proposal workloads");
    assert_eq!(fixture.synthetic_cases.len(), 24);
    assert_eq!(fixture.bgl.cases.len(), 4);

    for case in &fixture.synthetic_cases {
        validate_case(
            &case.id,
            &case.text,
            &case.pattern,
            &case.randomizer,
            &case.expected_match_positions,
        );
    }
    for case in &fixture.bgl.cases {
        validate_case(
            &case.id,
            &case.text,
            &case.pattern,
            &case.randomizer,
            &case.expected_match_positions,
        );
    }
}
