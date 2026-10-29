use cbpir_search::{
    SearchError, feer_fps_scores, feer_rfps_scores, fps_time_layout, rfps_time_layout,
    right_rotation_factors,
};
use num_complex::Complex64;

fn assert_close(actual: &[f64], expected: &[f64], tolerance: f64) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= tolerance,
            "index {index}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn fps_layout_uses_prefix_square_sentinel_and_reversed_pattern() {
    let layout = fps_time_layout(&[1, 2, 3, 4], &[2, 3]).expect("valid layout");

    assert_eq!(layout.transform_len, 8);
    assert_close(&layout.text, &[1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0], 0.0);
    assert_close(
        &layout.prefix_squares,
        &[1.0, 5.0, 14.0, 30.0, 0.0, 0.0, 0.0, 0.0],
        0.0,
    );
    assert_close(
        &layout.reversed_pattern,
        &[3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        0.0,
    );
    assert!((layout.pattern_square_sum - 13.0).abs() <= f64::EPSILON);
    assert_eq!(layout.valid_coefficients, 1..=3);
}

#[test]
fn right_rotation_factors_match_a_hand_derived_four_point_vector() {
    let expected = [
        Complex64::new(1.0, 0.0),
        Complex64::new(0.0, -1.0),
        Complex64::new(-1.0, 0.0),
        Complex64::new(0.0, 1.0),
    ];

    let actual = right_rotation_factors(4, 1);

    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).norm() <= 1e-12);
    }
}

#[test]
fn feer_fps_layout_reproduces_fixed_and_boundary_scores() {
    let scores = feer_fps_scores(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4]).expect("valid FPS");
    assert_close(&scores, &[3.0, 0.0, 11.0, 12.0, 3.0], 1e-9);

    let boundary = feer_fps_scores(&[1, 2, 1, 2, 3, 1, 2], &[1, 2]).expect("valid FPS");
    assert_close(&boundary, &[0.0, 2.0, 0.0, 2.0, 5.0, 0.0], 1e-9);
}

#[test]
fn feer_rfps_layout_reproduces_fixed_scores_and_cancellation_witness() {
    let layout = rfps_time_layout(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], &[2, 5, 3])
        .expect("valid RFPS layout");
    assert_eq!(layout.transform_len, 8);
    assert!((layout.pattern_randomizer_dot_product - 31.0).abs() <= f64::EPSILON);
    assert_eq!(layout.valid_coefficients, 2..=6);

    let scores =
        feer_rfps_scores(&[1, 2, 3, 4, 1, 2, 3], &[2, 3, 4], &[2, 5, 3]).expect("valid RFPS");
    assert_close(&scores, &[-10.0, 0.0, -2.0, -12.0, -10.0], 1e-9);

    let witness = feer_rfps_scores(&[2, 0, 4], &[0, 1, 3], &[2, 5, 1]).expect("valid RFPS witness");
    assert_close(&witness, &[0.0], 1e-9);
}

#[test]
fn layout_rejects_unsupported_lengths_and_randomizers() {
    assert_eq!(
        fps_time_layout(&[0; 1025], &[0]).unwrap().transform_len,
        2048
    );
    assert!(matches!(
        rfps_time_layout(&[1], &[1], &[0]),
        Err(SearchError::RandomizerOutsideSupportedDomain { index: 0, value: 0 })
    ));
    assert!(matches!(
        rfps_time_layout(&[1], &[1], &[33]),
        Err(SearchError::RandomizerOutsideSupportedDomain {
            index: 0,
            value: 33
        })
    ));
}
