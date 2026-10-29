use cbpir_search::{
    DecisionCause, MAX_PATTERN_LEN, PHASE1_FIXTURE_TEXT_LEN, ckks_rfps, classify_score,
};

#[test]
fn encrypted_rfps_reproduces_fixed_scores_and_true_match() {
    let result = ckks_rfps(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], &[2, 5, 3], 0.5)
        .expect("valid encrypted RFPS run");
    let expected = [-10.0, 0.0, -2.0, -12.0, -10.0];

    for (index, (actual, expected)) in result.scores.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= 0.02,
            "index {index}: expected {expected}, got {actual}"
        );
    }
    assert_eq!(result.matching_positions, vec![1]);
    assert_eq!(result.decisions[1].cause, DecisionCause::TrueMatchAccepted);
    assert!(result.max_abs_error <= 0.02);
    assert_eq!(result.metrics.ciphertext_multiplications, 1);
    assert_eq!(result.metrics.ciphertext_plaintext_multiplications, 0);
    assert_eq!(result.metrics.ciphertext_constant_multiplications, 1);
    assert_eq!(result.metrics.subtractions, 1);
    assert_eq!(result.metrics.scaling_technique, "FIXEDMANUAL");
    assert_eq!(result.metrics.explicit_rescaling_operations, 0);
    assert_eq!(result.metrics.output_state.level, 0);
    assert_eq!(result.metrics.output_state.noise_scale_degree, 2);
    let timings = result
        .metrics
        .phase_timings
        .expect("RFPS run records protocol phase timings");
    assert!(timings.context_and_key_generation_ns > 0);
    assert!(timings.client_setup_ns > 0);
    assert!(timings.client_query_ns > 0);
    assert!(timings.server_evaluation_ns > 0);
    assert!(timings.client_decrypt_idft_ns > 0);
}

#[test]
fn encrypted_rfps_identifies_intrinsic_cancellation() {
    let result = ckks_rfps(&[2, 0, 4], &[0, 1, 3], &[2, 5, 1], 0.5).expect("valid RFPS witness");

    assert_eq!(result.matching_positions, vec![0]);
    assert_eq!(result.exact_scores, vec![0]);
    assert_eq!(
        result.decisions[0].cause,
        DecisionCause::IntrinsicCancellation
    );
}

#[test]
fn encrypted_rfps_separates_broad_threshold_from_random_cancellation() {
    let result = ckks_rfps(&[2], &[0], &[1], 4.0).expect("valid broad-threshold RFPS");

    assert_eq!(result.exact_scores, vec![2]);
    assert_eq!(result.matching_positions, vec![0]);
    assert_eq!(result.decisions[0].cause, DecisionCause::ThresholdOnly);
}

#[test]
fn classification_separates_ckks_threshold_crossings() {
    assert_eq!(
        classify_score(false, 1, 0.49, 0.5),
        DecisionCause::CkksFalsePositiveThresholdCrossing
    );
    assert_eq!(
        classify_score(false, 0, 0.51, 0.5),
        DecisionCause::CkksFalseNegativeThresholdCrossing
    );
    assert_eq!(
        classify_score(true, 0, 0.51, 0.5),
        DecisionCause::TrueMatchRejectedByCkks
    );
}

#[test]
fn encrypted_rfps_accepts_phase1_text_fixture() {
    let mut text = vec![0_u8; PHASE1_FIXTURE_TEXT_LEN];
    let pattern = (128_u8..192).collect::<Vec<_>>();
    let randomizer = vec![3_u8; MAX_PATTERN_LEN];
    let position = PHASE1_FIXTURE_TEXT_LEN - MAX_PATTERN_LEN;
    text[position..].copy_from_slice(&pattern);

    let result =
        ckks_rfps(&text, &pattern, &randomizer, 0.5).expect("valid Phase-1 RFPS fixture run");

    assert_eq!(result.matching_positions, vec![position]);
    assert!(
        result.max_abs_error <= 2.0,
        "Phase-1 RFPS fixture error was {}",
        result.max_abs_error
    );
    assert_eq!(result.metrics.batch_size, 2048);
}

#[test]
fn encrypted_rfps_preserves_boundary_and_multiple_true_matches() {
    let result =
        ckks_rfps(&[7, 8, 1, 2, 7, 8], &[7, 8], &[3, 4], 0.5).expect("valid boundary RFPS run");
    assert_eq!(result.matching_positions, vec![0, 4]);

    let overlapping =
        ckks_rfps(&[1, 1, 1, 1], &[1, 1], &[3, 4], 0.5).expect("valid overlapping RFPS run");
    assert_eq!(overlapping.matching_positions, vec![0, 1, 2]);
}

#[test]
fn encrypted_rfps_handles_declared_extreme_byte_scores() {
    let maximum = vec![u8::MAX; MAX_PATTERN_LEN];
    let zero = vec![0_u8; MAX_PATTERN_LEN];
    let randomizer = vec![32_u8; MAX_PATTERN_LEN];

    let true_match =
        ckks_rfps(&maximum, &maximum, &randomizer, 4.0).expect("extreme RFPS true match");
    assert_eq!(true_match.exact_scores, vec![0]);
    assert_eq!(true_match.matching_positions, vec![0]);

    let mismatch = ckks_rfps(&maximum, &zero, &randomizer, 4.0).expect("extreme RFPS mismatch");
    assert_eq!(mismatch.exact_scores, vec![522_240]);
    assert!(mismatch.matching_positions.is_empty());
    assert!(mismatch.scores[0].is_finite());
}
