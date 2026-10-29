use cbpir_search::{
    CkksError, MAX_PATTERN_LEN, PHASE1_FIXTURE_TEXT_LEN, PHASE1_FIXTURE_TRANSFORM_LEN,
    RFPS_SCALING_MODULUS_BITS, SearchError, ckks_fps, ckks_rfps,
    ckks_rfps_with_scaling_modulus_bits, fps_time_layout,
};

#[test]
fn phase1_fixture_constants_do_not_define_runtime_capacity() {
    assert_eq!(PHASE1_FIXTURE_TEXT_LEN, 1024);
    assert_eq!(MAX_PATTERN_LEN, 64);
    assert_eq!(PHASE1_FIXTURE_TRANSFORM_LEN, 2048);

    let layout = fps_time_layout(&[255; PHASE1_FIXTURE_TEXT_LEN], &[255; MAX_PATTERN_LEN])
        .expect("retained Phase-1 fixture layout");
    assert_eq!(layout.transform_len, PHASE1_FIXTURE_TRANSFORM_LEN);
    assert_eq!(layout.valid_coefficients, 63..=1023);
}

#[test]
fn text_beyond_phase1_fixture_is_valid_but_pattern_overflow_is_typed() {
    assert_eq!(
        fps_time_layout(&vec![0; PHASE1_FIXTURE_TEXT_LEN + 1], &[0])
            .unwrap()
            .transform_len,
        2048
    );
    assert_eq!(
        fps_time_layout(&[0; PHASE1_FIXTURE_TEXT_LEN], &[0; MAX_PATTERN_LEN + 1]),
        Err(SearchError::PatternLengthExceeded {
            actual: MAX_PATTERN_LEN + 1,
            maximum: MAX_PATTERN_LEN,
        })
    );
}

#[test]
fn retained_parameters_preserve_maximum_magnitude_decisions() {
    let maximum_text = vec![u8::MAX; PHASE1_FIXTURE_TEXT_LEN];
    let maximum_pattern = vec![u8::MAX; MAX_PATTERN_LEN];
    let zero_pattern = vec![0_u8; MAX_PATTERN_LEN];
    let randomizer = vec![32_u8; MAX_PATTERN_LEN];
    let all_positions = (0..=PHASE1_FIXTURE_TEXT_LEN - MAX_PATTERN_LEN).collect::<Vec<_>>();

    let fps_true =
        ckks_fps(&maximum_text, &maximum_pattern, 0.5).expect("maximum-magnitude FPS true match");
    assert!(
        fps_true
            .exact_scores
            .iter()
            .all(|score| score.abs() <= f64::EPSILON)
    );
    assert_eq!(fps_true.matching_positions, all_positions);
    assert_eq!(fps_true.metrics.batch_size, 2048);

    let fps_mismatch = ckks_fps(&maximum_text, &zero_pattern, 0.5).expect("maximum FPS mismatch");
    assert!(
        fps_mismatch
            .exact_scores
            .iter()
            .all(|score| (*score - 4_161_600.0).abs() <= f64::EPSILON)
    );
    assert!(fps_mismatch.matching_positions.is_empty());

    let rfps_true = ckks_rfps(&maximum_text, &maximum_pattern, &randomizer, 4.0)
        .expect("maximum-magnitude RFPS true match");
    assert!(rfps_true.exact_scores.iter().all(|score| *score == 0));
    assert_eq!(rfps_true.matching_positions, all_positions);
    assert_eq!(rfps_true.metrics.batch_size, 2048);

    let rfps_mismatch =
        ckks_rfps(&maximum_text, &zero_pattern, &randomizer, 4.0).expect("maximum RFPS mismatch");
    assert!(
        rfps_mismatch
            .exact_scores
            .iter()
            .all(|score| *score == 522_240)
    );
    assert!(rfps_mismatch.matching_positions.is_empty());
}

#[test]
fn explicit_rfps_scale_probe_keeps_default_mapping_reproducible() {
    let explicit =
        ckks_rfps_with_scaling_modulus_bits(&[1, 2, 3], &[2], &[3], 0.5, RFPS_SCALING_MODULUS_BITS)
            .expect("explicit retained RFPS scale");
    assert_eq!(
        explicit.metrics.scaling_modulus_bits,
        RFPS_SCALING_MODULUS_BITS
    );
    assert_eq!(
        ckks_rfps_with_scaling_modulus_bits(&[1], &[1], &[3], 0.5, 0),
        Err(CkksError::UnsupportedScalingModulusBits(0))
    );
    assert_eq!(
        ckks_rfps_with_scaling_modulus_bits(&[1], &[1], &[3], 0.5, 61),
        Err(CkksError::UnsupportedScalingModulusBits(61))
    );
}
