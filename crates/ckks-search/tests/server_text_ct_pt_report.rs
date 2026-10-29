use cbpir_search::server_text_ct_pt_focused_report;

#[test]
fn focused_report_names_backends_and_preserves_negative_classifications() {
    let report = server_text_ct_pt_focused_report(1).unwrap();

    assert_eq!(report["schema"], "p0c-server-text-ct-pt-v1");
    assert_eq!(report["gate_passed"], true);
    assert_eq!(report["variant_backend"], "server_text_ct_pt");
    assert_eq!(
        report["operation_audit"]["server_text_ct_pt"]["fps"]["ciphertext_multiplications"],
        0
    );
    assert_eq!(
        report["operation_audit"]["server_text_ct_pt"]["rfps"]["ciphertext_multiplications"],
        0
    );
    assert_eq!(report["validation"]["fps"]["positions_agree"], true);
    assert_eq!(
        report["validation"]["rfps_cancellation"]["classification"],
        "IntrinsicCancellation"
    );
    assert_eq!(
        report["validation"]["rfps_threshold_only"]["classification"],
        "ThresholdOnly"
    );
    assert_eq!(report["validation"]["packed_rfps"]["trial_isolation"], true);
}
