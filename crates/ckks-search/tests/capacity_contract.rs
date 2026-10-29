use cbpir_search::{
    CkksConfig, CkksError, FpsProfile, RfpsProfile, fps_time_layout, rfps_time_layout,
};

#[test]
fn layout_has_no_legacy_1024_byte_runtime_ceiling() {
    let text = vec![7_u8; 4_095];
    let pattern = vec![7_u8; 64];
    let randomizer = vec![3_u8; pattern.len()];

    assert_eq!(
        fps_time_layout(&text, &pattern).unwrap().transform_len,
        4_096
    );
    assert_eq!(
        rfps_time_layout(&text, &pattern, &randomizer)
            .unwrap()
            .transform_len,
        4_096
    );
}

#[test]
fn named_profiles_accept_full_slots_and_reject_smallest_over_capacity_text() {
    for (profile, slots) in [
        (FpsProfile::Fps13, 4_096),
        (FpsProfile::Fps14, 8_192),
        (FpsProfile::Fps15, 16_384),
    ] {
        assert!(profile.config_for_transform_len(slots).is_ok());
        assert_eq!(
            profile.config_for_transform_len(slots * 2),
            Err(CkksError::TransformExceedsProfileCapacity {
                transform_len: slots * 2,
                available_slots: slots,
            })
        );
    }

    for (profile, slots) in [
        (RfpsProfile::Rfps13, 4_096),
        (RfpsProfile::Rfps14, 8_192),
        (RfpsProfile::Rfps15, 16_384),
    ] {
        assert!(profile.config_for_transform_len(slots).is_ok());
        assert_eq!(
            profile.config_for_transform_len(slots * 2),
            Err(CkksError::TransformExceedsProfileCapacity {
                transform_len: slots * 2,
                available_slots: slots,
            })
        );
    }
}

#[test]
fn default_fps_is_fps14_and_default_rfps_is_rfps13() {
    let fps = CkksConfig::for_fps_transform_len(8_192).unwrap();
    assert_eq!(fps.ring_dimension, Some(16_384));
    assert_eq!(fps.scaling_modulus_bits, 40);
    assert_eq!(
        CkksConfig::for_fps_transform_len(16_384),
        Err(CkksError::TransformExceedsProfileCapacity {
            transform_len: 16_384,
            available_slots: 8_192,
        })
    );

    let rfps = CkksConfig::for_rfps_transform_len(4_096).unwrap();
    assert_eq!(rfps.ring_dimension, Some(8_192));
    assert_eq!(rfps.scaling_modulus_bits, 24);
    assert_eq!(
        CkksConfig::for_rfps_transform_len(8_192),
        Err(CkksError::TransformExceedsProfileCapacity {
            transform_len: 8_192,
            available_slots: 4_096,
        })
    );
}
