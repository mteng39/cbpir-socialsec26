use cbpir_search::{CkksConfig, RFPS_SCALING_MODULUS_BITS, ckks_componentwise_product};
use num_complex::Complex64;

#[test]
fn ckks_config_records_the_direct_complex_execution_contract() {
    let fps = CkksConfig::for_fps_transform_len(8).expect("supported FPS transform length");
    let rfps = CkksConfig::for_rfps_transform_len(8).expect("supported RFPS transform length");

    assert_eq!(fps.batch_size, 8);
    assert_eq!(fps.multiplicative_depth, 2);
    assert_eq!(fps.scaling_modulus_bits, 40);
    assert_eq!(fps.first_modulus_bits, 60);
    assert!((fps.decision_threshold - 0.5).abs() <= f64::EPSILON);

    assert_eq!(rfps.batch_size, 8);
    assert_eq!(rfps.multiplicative_depth, 1);
    assert_eq!(rfps.scaling_modulus_bits, RFPS_SCALING_MODULUS_BITS);
    assert_eq!(rfps.first_modulus_bits, 60);
    assert!((rfps.decision_threshold - 4.0).abs() <= f64::EPSILON);
}

#[test]
fn ckks_componentwise_product_matches_independent_complex_arithmetic() {
    let left = [
        Complex64::new(1.0, 2.0),
        Complex64::new(-3.0, 0.5),
        Complex64::new(0.25, -4.0),
        Complex64::new(2.0, -1.0),
    ];
    let right = [
        Complex64::new(0.5, -1.0),
        Complex64::new(2.0, 3.0),
        Complex64::new(-2.0, 0.75),
        Complex64::new(1.5, 2.5),
    ];
    let expected = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .collect::<Vec<_>>();

    let result = ckks_componentwise_product(&left, &right).expect("valid CKKS product");

    assert_eq!(result.values.len(), expected.len());
    let max_error = result
        .values
        .iter()
        .zip(expected)
        .map(|(actual, expected)| (actual - expected).norm())
        .fold(0.0_f64, f64::max);
    assert!(max_error <= 1e-4, "maximum component error was {max_error}");
    assert_eq!(result.metrics.ciphertext_multiplications, 1);
    assert_eq!(result.metrics.ciphertext_plaintext_multiplications, 0);
    assert_eq!(result.metrics.openfhe_version, "1.5.1");
    assert_eq!(result.metrics.reported_log_precision_bits, None);
}
