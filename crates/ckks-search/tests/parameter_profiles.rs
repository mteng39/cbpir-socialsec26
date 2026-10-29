use cbpir_search::{
    FpsCkksParameters, FpsKeySwitchTechnique, FpsProfile, FpsScalingTechnique,
    PHASE1_FIXTURE_TEXT_LEN, RfpsProfile, server_text_ct_pt_fps_with_parameters,
    server_text_ct_pt_fps_with_profile, server_text_ct_pt_rfps_with_profile,
};

#[test]
fn fps_repair_parameters_start_from_openfhe_defaults() {
    let recommended = FpsCkksParameters::openfhe_recommended(32_768, 8).unwrap();
    assert_eq!(recommended.ring_dimension, 32_768);
    assert_eq!(recommended.multiplicative_depth, 2);
    assert_eq!(recommended.first_modulus_bits, 60);
    assert_eq!(recommended.scaling_modulus_bits, 50);
    assert_eq!(
        recommended.scaling_technique,
        FpsScalingTechnique::FlexibleAutoExt
    );
    assert_eq!(
        recommended.key_switch_technique,
        FpsKeySwitchTechnique::Hybrid
    );
}

#[test]
fn explicit_fps_parameters_keep_the_retained_operation_path() {
    let parameters =
        FpsCkksParameters::fixed_manual(16_384, 8, 60, 40, FpsKeySwitchTechnique::Hybrid).unwrap();
    let result =
        server_text_ct_pt_fps_with_parameters(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], 0.5, parameters)
            .unwrap();
    assert_eq!(result.matching_positions, vec![1]);
    assert_eq!(result.metrics.ring_dimension, 16_384);
    assert_eq!(result.metrics.operations.ciphertext_multiplications, 0);
    assert_eq!(
        result
            .metrics
            .operations
            .ciphertext_plaintext_multiplications,
        1
    );
    assert_eq!(result.metrics.operations.explicit_rescaling_operations, 1);
}

#[test]
fn named_ckks_profiles_preserve_requested_parameters() {
    for (profile, log_n, scaling_bits, key_switch) in [
        (FpsProfile::Fps13, 13, 50, FpsKeySwitchTechnique::Bv),
        (FpsProfile::Fps14, 14, 40, FpsKeySwitchTechnique::Hybrid),
        (FpsProfile::Fps15, 15, 50, FpsKeySwitchTechnique::Hybrid),
    ] {
        let config = profile.config_for_transform_len(8).unwrap();
        let parameters = profile.parameters_for_transform_len(8).unwrap();
        assert_eq!(profile.log_n(), log_n);
        assert_eq!(config.ring_dimension, Some(1_u32 << log_n));
        assert_eq!(config.multiplicative_depth, 2);
        assert_eq!(config.first_modulus_bits, 60);
        assert_eq!(config.scaling_modulus_bits, scaling_bits);
        assert_eq!(config.key_switch_technique, key_switch);
        assert_eq!(config.scaling_technique, FpsScalingTechnique::FixedManual);
        assert_eq!(parameters.key_switch_technique, key_switch);
    }

    for (profile, log_n) in [
        (RfpsProfile::Rfps13, 13),
        (RfpsProfile::Rfps14, 14),
        (RfpsProfile::Rfps15, 15),
    ] {
        let config = profile.config_for_transform_len(8).unwrap();
        assert_eq!(profile.log_n(), log_n);
        assert_eq!(config.ring_dimension, Some(1_u32 << log_n));
        assert_eq!(config.multiplicative_depth, 1);
        assert_eq!(config.first_modulus_bits, 60);
        assert_eq!(config.scaling_modulus_bits, 24);
    }
}

#[test]
fn all_ckks_profiles_run_retained_server_text_match_paths() {
    for profile in [FpsProfile::Fps13, FpsProfile::Fps14, FpsProfile::Fps15] {
        let result =
            server_text_ct_pt_fps_with_profile(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], 0.5, profile)
                .unwrap();
        assert_eq!(result.matching_positions, vec![1]);
        assert_eq!(result.metrics.ring_dimension, profile.ring_dimension());
    }

    for profile in [
        RfpsProfile::Rfps13,
        RfpsProfile::Rfps14,
        RfpsProfile::Rfps15,
    ] {
        let result = server_text_ct_pt_rfps_with_profile(
            &[1, 2, 3, 4, 1, 2, 3],
            &[2, 3, 4],
            &[2, 5, 3],
            0.5,
            profile,
        )
        .unwrap();
        assert_eq!(result.matching_positions, vec![1]);
        assert_eq!(result.metrics.ring_dimension, profile.ring_dimension());
    }
}

#[test]
fn repaired_named_fps_profiles_pass_the_retained_phase1_fixture() {
    let pattern = (128_u8..192).collect::<Vec<_>>();
    let mut text = vec![0_u8; PHASE1_FIXTURE_TEXT_LEN];
    let position = text.len() - pattern.len();
    text[position..].copy_from_slice(&pattern);

    for profile in [FpsProfile::Fps13, FpsProfile::Fps14, FpsProfile::Fps15] {
        let result = server_text_ct_pt_fps_with_profile(&text, &pattern, 0.5, profile).unwrap();
        assert_eq!(result.matching_positions, vec![position]);
        assert!(result.max_abs_error <= 1.0, "{profile:?}");
        assert_eq!(result.metrics.operations.ciphertext_multiplications, 0);
        assert_eq!(result.metrics.operations.relinearizations, 0);
    }
}
