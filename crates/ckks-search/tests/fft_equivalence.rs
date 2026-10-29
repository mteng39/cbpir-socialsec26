use cbpir_search::{dft, idft, naive_dft_reference, naive_idft_reference};
use num_complex::Complex64;

fn max_error(left: &[Complex64], right: &[Complex64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| (*left - *right).norm())
        .fold(0.0, f64::max)
}

#[test]
fn fft_matches_naive_dft_and_idft_at_audit_lengths() {
    for length in [64, 256, 1_024, 2_048] {
        let values = (0..length)
            .map(|index| {
                Complex64::new(
                    ((index * 17 + 3) % 251) as f64 - 125.0,
                    ((index * 29 + 7) % 127) as f64 - 63.0,
                )
            })
            .collect::<Vec<_>>();
        let expected_dft = naive_dft_reference(&values);
        let actual_dft = dft(&values);
        assert!(max_error(&expected_dft, &actual_dft) <= 1e-6);

        let expected_idft = naive_idft_reference(&expected_dft);
        let actual_idft = idft(&actual_dft);
        assert!(max_error(&expected_idft, &actual_idft) <= 1e-8);
    }
}

#[test]
fn empty_transform_semantics_are_preserved() {
    assert!(dft(&[]).is_empty());
    assert!(idft(&[]).is_empty());
    assert!(naive_dft_reference(&[]).is_empty());
    assert!(naive_idft_reference(&[]).is_empty());
}
