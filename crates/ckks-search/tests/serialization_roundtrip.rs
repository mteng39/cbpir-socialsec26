use cbpir_search::{ckks_fps_serialization_roundtrip, ckks_rfps_serialization_roundtrip};

fn assert_common_sizes(
    context: u64,
    public_key: u64,
    private_key: u64,
    eval_mult_key: u64,
    response: u64,
) {
    for (name, size) in [
        ("context", context),
        ("public key", public_key),
        ("private key", private_key),
        ("evaluation multiplication key", eval_mult_key),
        ("response", response),
    ] {
        assert!(size > 0, "serialized {name} was empty");
    }
}

#[test]
fn fps_binary_reload_preserves_search_and_protocol_artifact_counts() {
    let result = ckks_fps_serialization_roundtrip(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], 0.5)
        .expect("valid FPS serialization round trip");

    assert_eq!(result.matching_positions, vec![1]);
    assert!(result.max_abs_error <= 1e-3);
    assert_eq!(result.sizes.setup_ciphertext_bytes.len(), 2);
    assert_eq!(result.sizes.query_ciphertext_bytes.len(), 2);
    assert!(
        result
            .sizes
            .setup_ciphertext_bytes
            .iter()
            .all(|size| *size > 0)
    );
    assert!(
        result
            .sizes
            .query_ciphertext_bytes
            .iter()
            .all(|size| *size > 0)
    );
    assert_common_sizes(
        result.sizes.context_bytes,
        result.sizes.public_key_bytes,
        result.sizes.private_key_bytes,
        result.sizes.eval_mult_key_bytes,
        result.sizes.response_ciphertext_bytes,
    );
}

#[test]
fn rfps_binary_reload_preserves_search_and_protocol_artifact_counts() {
    let result =
        ckks_rfps_serialization_roundtrip(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], &[2, 5, 3], 0.5)
            .expect("valid RFPS serialization round trip");

    assert_eq!(result.matching_positions, vec![1]);
    assert!(result.max_abs_error <= 0.02);
    assert_eq!(result.sizes.setup_ciphertext_bytes.len(), 1);
    assert_eq!(result.sizes.query_ciphertext_bytes.len(), 2);
    assert!(
        result
            .sizes
            .setup_ciphertext_bytes
            .iter()
            .all(|size| *size > 0)
    );
    assert!(
        result
            .sizes
            .query_ciphertext_bytes
            .iter()
            .all(|size| *size > 0)
    );
    assert_common_sizes(
        result.sizes.context_bytes,
        result.sizes.public_key_bytes,
        result.sizes.private_key_bytes,
        result.sizes.eval_mult_key_bytes,
        result.sizes.response_ciphertext_bytes,
    );
}

#[test]
fn consecutive_same_config_probes_do_not_accumulate_evaluation_keys() {
    let first =
        ckks_fps_serialization_roundtrip(&[1, 2, 3], &[2], 0.5).expect("first serialization probe");
    let second = ckks_fps_serialization_roundtrip(&[1, 2, 3], &[2], 0.5)
        .expect("second serialization probe");

    assert_eq!(
        first.sizes.eval_mult_key_bytes, second.sizes.eval_mult_key_bytes,
        "same-parameter sessions must serialize one evaluation-key set, not accumulated global keys"
    );
}
