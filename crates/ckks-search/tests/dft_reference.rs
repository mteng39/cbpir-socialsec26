use cbpir_search::{dft, idft};
use num_complex::Complex64;

#[test]
fn negative_exponent_dft_matches_independently_derived_vector() {
    let input = [1.0, 2.0, 3.0, 4.0].map(|value| Complex64::new(value, 0.0));
    let expected = [
        Complex64::new(10.0, 0.0),
        Complex64::new(-2.0, 2.0),
        Complex64::new(-2.0, 0.0),
        Complex64::new(-2.0, -2.0),
    ];

    let actual = dft(&input);

    assert_eq!(actual.len(), expected.len());
    for (coefficient, expected_coefficient) in actual.iter().zip(expected) {
        assert!((coefficient - expected_coefficient).norm() <= 1e-12);
    }
}

#[test]
fn positive_exponent_normalized_idft_matches_independently_derived_input() {
    let transformed = [
        Complex64::new(10.0, 0.0),
        Complex64::new(-2.0, 2.0),
        Complex64::new(-2.0, 0.0),
        Complex64::new(-2.0, -2.0),
    ];
    let expected = [1.0, 2.0, 3.0, 4.0].map(|value| Complex64::new(value, 0.0));

    let actual = idft(&transformed);

    assert_eq!(actual.len(), expected.len());
    for (coefficient, expected_coefficient) in actual.iter().zip(expected) {
        assert!((coefficient - expected_coefficient).norm() <= 1e-12);
    }
}
