use cbpir_search::{
    RfpsPackedTrialInput, server_text_ct_pt_fps_artifact_sizes,
    server_text_ct_pt_rfps_artifact_sizes, server_text_ct_pt_rfps_packed_artifact_sizes,
};

#[test]
fn server_text_ct_pt_artifact_sizes_separate_query_response_and_keys() {
    let fps = server_text_ct_pt_fps_artifact_sizes(&[1, 2, 3, 4], &[2, 3], 0.5).unwrap();
    assert!(fps.context_bytes > 0);
    assert!(fps.public_key_bytes > 0);
    assert!(fps.secret_key_bytes > 0);
    assert_eq!(fps.evaluation_multiplication_key_bytes, None);
    assert_eq!(fps.rotation_key_bytes, None);
    assert_eq!(fps.encrypted_query_ciphertext_bytes.len(), 2);
    assert!(
        fps.encrypted_query_ciphertext_bytes
            .iter()
            .all(|bytes| *bytes > 0)
    );
    assert!(fps.encrypted_response_ciphertext_bytes > 0);
    assert_eq!(fps.encoded_server_plaintext_bytes, None);
    assert!(fps.plaintext_search_state_bytes > 0);

    let rfps =
        server_text_ct_pt_rfps_artifact_sizes(&[0, 2, 1], &[2, 0, 1], &[3, 5, 7], 0.5).unwrap();
    assert_eq!(rfps.evaluation_multiplication_key_bytes, None);
    assert_eq!(rfps.encrypted_query_ciphertext_bytes.len(), 2);
    assert!(rfps.encrypted_response_ciphertext_bytes > 0);

    let packed = server_text_ct_pt_rfps_packed_artifact_sizes(
        &[
            RfpsPackedTrialInput {
                id: "a".to_owned(),
                text: vec![0, 2, 1],
                pattern: vec![2, 0, 1],
                randomizer: vec![3, 5, 7],
            },
            RfpsPackedTrialInput {
                id: "b".to_owned(),
                text: vec![2, 1, 0],
                pattern: vec![2, 1, 0],
                randomizer: vec![11, 13, 17],
            },
        ],
        0.5,
    )
    .unwrap();
    assert_eq!(packed.evaluation_multiplication_key_bytes, None);
    assert_eq!(packed.encrypted_query_ciphertext_bytes.len(), 2);
    assert!(packed.encrypted_response_ciphertext_bytes > 0);
}
