use cbpir_search::{MAX_PATTERN_LEN, PHASE1_FIXTURE_TEXT_LEN, ckks_fps};

#[test]
fn encrypted_fps_reproduces_fixed_p0b_scores_and_matches() {
    let result =
        ckks_fps(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], 0.5).expect("valid encrypted FPS run");
    let expected = [3.0, 0.0, 11.0, 12.0, 3.0];

    assert_eq!(result.scores.len(), expected.len());
    for (index, (actual, expected)) in result.scores.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= 1e-3,
            "index {index}: expected {expected}, got {actual}"
        );
    }
    assert_eq!(result.matching_positions, vec![1]);
    assert!(result.max_abs_error <= 1e-3);
    assert_eq!(result.metrics.ciphertext_multiplications, 1);
    assert_eq!(result.metrics.ciphertext_plaintext_multiplications, 1);
    assert_eq!(result.metrics.ciphertext_constant_multiplications, 1);
    assert_eq!(result.metrics.additions, 2);
    assert_eq!(result.metrics.subtractions, 2);
    assert_eq!(result.metrics.scaling_technique, "FIXEDMANUAL");
    assert_eq!(result.metrics.explicit_rescaling_operations, 1);
    assert_eq!(result.metrics.output_state.level, 1);
    assert_eq!(result.metrics.output_state.noise_scale_degree, 1);
    let timings = result
        .metrics
        .phase_timings
        .expect("FPS run records protocol phase timings");
    assert!(timings.context_and_key_generation_ns > 0);
    assert!(timings.client_setup_ns > 0);
    assert!(timings.client_query_ns > 0);
    assert!(timings.server_evaluation_ns > 0);
    assert!(timings.client_decrypt_idft_ns > 0);
}

#[test]
fn encrypted_fps_preserves_overlapping_and_boundary_matches() {
    let result = ckks_fps(&[1, 1, 1, 1], &[1, 1], 0.5).expect("valid overlapping FPS run");
    assert_eq!(result.matching_positions, vec![0, 1, 2]);

    let boundary = ckks_fps(&[7, 8, 1, 2, 7, 8], &[7, 8], 0.5).expect("valid boundary FPS run");
    assert_eq!(boundary.matching_positions, vec![0, 4]);
}

#[test]
fn encrypted_fps_accepts_phase1_text_fixture() {
    let mut text = vec![0_u8; PHASE1_FIXTURE_TEXT_LEN];
    let pattern = (128_u8..192).collect::<Vec<_>>();
    let position = PHASE1_FIXTURE_TEXT_LEN - MAX_PATTERN_LEN;
    text[position..].copy_from_slice(&pattern);

    let result = ckks_fps(&text, &pattern, 0.5).expect("valid Phase-1 FPS fixture run");

    assert_eq!(result.matching_positions, vec![position]);
    assert!(
        result.max_abs_error <= 1.0,
        "Phase-1 FPS fixture error was {}",
        result.max_abs_error
    );
    assert_eq!(result.metrics.batch_size, 2048);
}

#[test]
fn encrypted_fps_handles_declared_extreme_byte_scores() {
    let maximum = vec![u8::MAX; MAX_PATTERN_LEN];
    let zero = vec![0_u8; MAX_PATTERN_LEN];

    let true_match = ckks_fps(&maximum, &maximum, 0.5).expect("valid extreme FPS true match");
    assert_eq!(true_match.exact_scores, vec![0.0]);
    assert_eq!(
        true_match.matching_positions,
        vec![0],
        "extreme true-match score was {}",
        true_match.scores[0]
    );

    let mismatch = ckks_fps(&maximum, &zero, 0.5).expect("valid extreme FPS mismatch");
    assert_eq!(mismatch.exact_scores, vec![4_161_600.0]);
    assert!(mismatch.matching_positions.is_empty());
    assert!(mismatch.scores[0].is_finite());
}
