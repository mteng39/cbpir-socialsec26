use num_complex::Complex64;
use rustfft::FftPlanner;
use std::f64::consts::TAU;

/// Computes the mathematical DFT with a negative exponent and no normalization.
#[must_use]
pub fn dft(values: &[Complex64]) -> Vec<Complex64> {
    if values.is_empty() {
        return Vec::new();
    }
    let mut transformed = values.to_vec();
    FftPlanner::<f64>::new()
        .plan_fft_forward(transformed.len())
        .process(&mut transformed);
    transformed
}

/// Computes the mathematical IDFT with a positive exponent and `1/N` normalization.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn idft(values: &[Complex64]) -> Vec<Complex64> {
    if values.is_empty() {
        return Vec::new();
    }
    let mut transformed = values.to_vec();
    FftPlanner::<f64>::new()
        .plan_fft_inverse(transformed.len())
        .process(&mut transformed);
    let normalization = transformed.len() as f64;
    for value in &mut transformed {
        *value /= normalization;
    }
    transformed
}

/// Direct `O(N^2)` DFT retained only as a numerical reference for tests/audits.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn naive_dft_reference(values: &[Complex64]) -> Vec<Complex64> {
    let length = values.len();
    (0..length)
        .map(|frequency| {
            values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    let angle = -TAU * (frequency * index) as f64 / length as f64;
                    value * Complex64::from_polar(1.0, angle)
                })
                .sum()
        })
        .collect()
}

/// Direct `O(N^2)` IDFT retained only as a numerical reference for tests/audits.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn naive_idft_reference(values: &[Complex64]) -> Vec<Complex64> {
    let length = values.len();
    (0..length)
        .map(|index| {
            values
                .iter()
                .enumerate()
                .map(|(frequency, value)| {
                    let angle = TAU * (frequency * index) as f64 / length as f64;
                    value * Complex64::from_polar(1.0, angle)
                })
                .sum::<Complex64>()
                / length as f64
        })
        .collect()
}
