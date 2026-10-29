use cbpir_search::{
    CkksError, DecisionCause, SearchError, feer_fps_scores, feer_rfps_scores,
    server_text_ct_pt_fps, server_text_ct_pt_rfps,
};

#[test]
fn server_text_ct_pt_fps_preserves_scores_without_ciphertext_products() {
    let result = server_text_ct_pt_fps(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], 0.5)
        .expect("valid server-text FPS run");

    assert_eq!(result.exact_scores, vec![3.0, 0.0, 11.0, 12.0, 3.0]);
    assert_eq!(result.matching_positions, vec![1]);
    assert!(result.max_abs_error <= 1e-3);
    assert_eq!(result.metrics.backend, "server_text_ct_pt");
    assert_eq!(result.metrics.operations.ciphertext_multiplications, 0);
    assert_eq!(
        result
            .metrics
            .operations
            .ciphertext_plaintext_multiplications,
        1
    );
    assert_eq!(result.metrics.operations.relinearizations, 0);
    assert_eq!(result.metrics.operations.explicit_rescaling_operations, 1);
    assert!(!result.metrics.keys.evaluation_multiplication_key);
    assert!(!result.metrics.keys.relinearization_key);
    assert!(!result.metrics.keys.rotation_keys);
    assert!(result.metrics.keys.public_key);
    assert!(result.metrics.keys.secret_key);
    assert!(result.metrics.timings.server_text_dft_preparation_ns > 0);
    assert!(result.metrics.timings.server_plaintext_encoding_ns > 0);
    assert!(result.metrics.timings.client_query_encryption_ns > 0);
    assert!(result.metrics.timings.server_homomorphic_evaluation_ns > 0);
    assert!(result.metrics.timings.client_decrypt_ns > 0);
    assert!(result.metrics.timings.client_idft_ns > 0);
}

#[test]
fn server_text_ct_pt_fps_matches_independent_oracle_at_boundaries_and_overlaps() {
    for (text, pattern, expected_positions) in [
        (&b"AAAA"[..], &b"AA"[..], vec![0, 1, 2]),
        (&b"needle-other-needle"[..], &b"needle"[..], vec![0, 13]),
    ] {
        let result = server_text_ct_pt_fps(text, pattern, 0.5).unwrap();
        let oracle = feer_fps_scores(text, pattern).unwrap();
        assert_eq!(result.matching_positions, expected_positions);
        for (index, (actual, exact)) in result.scores.iter().zip(oracle).enumerate() {
            assert!(
                (actual - exact).abs() <= 1.0,
                "text={text:?} pattern={pattern:?} index={index} actual={actual} exact={exact} error={}",
                (actual - exact).abs()
            );
        }
    }
}

#[test]
fn server_text_ct_pt_rfps_preserves_classification_without_ciphertext_products() {
    let result = server_text_ct_pt_rfps(&[0, 2, 1], &[2, 0, 1], &[3, 5, 7], 0.5)
        .expect("valid server-text RFPS run");

    assert_eq!(result.exact_scores, vec![4]);
    assert!((result.scores[0] - 4.0).abs() <= 1.0);
    assert_eq!(result.decisions.len(), 1);
    assert_eq!(result.metrics.backend, "server_text_ct_pt");
    assert_eq!(result.metrics.operations.ciphertext_multiplications, 0);
    assert_eq!(
        result
            .metrics
            .operations
            .ciphertext_plaintext_multiplications,
        1
    );
    assert_eq!(
        result
            .metrics
            .operations
            .ciphertext_constant_multiplications,
        1
    );
    assert_eq!(result.metrics.operations.subtractions, 1);
    assert_eq!(result.metrics.operations.relinearizations, 0);
    assert_eq!(result.metrics.operations.explicit_rescaling_operations, 0);
    assert!(!result.metrics.keys.evaluation_multiplication_key);
}

#[test]
fn server_text_ct_pt_rfps_retains_cancellation_and_threshold_only_semantics() {
    let cancellation = server_text_ct_pt_rfps(&[2, 0], &[1, 1], &[1, 1], 0.5).unwrap();
    assert_eq!(cancellation.exact_scores, vec![0]);
    assert_eq!(
        cancellation.decisions[0].cause,
        DecisionCause::IntrinsicCancellation
    );

    let threshold_only = server_text_ct_pt_rfps(&[2], &[1], &[3], 4.0).unwrap();
    assert_eq!(threshold_only.exact_scores, vec![3]);
    assert_eq!(
        threshold_only.decisions[0].cause,
        DecisionCause::ThresholdOnly
    );
    let oracle = feer_rfps_scores(&[2], &[1], &[3]).unwrap();
    assert!((threshold_only.scores[0] - oracle[0]).abs() <= 1.0);
}

#[test]
fn server_text_ct_pt_scalar_reuses_baseline_validation_contract() {
    assert_eq!(
        server_text_ct_pt_fps(&[1], &[], 0.5),
        Err(CkksError::Search(SearchError::EmptyPattern))
    );
    assert_eq!(
        server_text_ct_pt_rfps(&[1], &[1], &[0], 0.5),
        Err(CkksError::Search(
            SearchError::RandomizerOutsideSupportedDomain { index: 0, value: 0 }
        ))
    );
    assert_eq!(
        server_text_ct_pt_fps(&[1], &[1], f64::NAN),
        Err(CkksError::InvalidDecisionThreshold)
    );
}
