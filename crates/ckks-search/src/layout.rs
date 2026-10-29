use crate::{dft, idft};
use num_complex::Complex64;
use std::f64::consts::TAU;
use std::ops::RangeInclusive;

/// Text length used by the retained Phase-1 boundary fixture.
///
/// This is evidence metadata, not a runtime capacity limit.
pub const PHASE1_FIXTURE_TEXT_LEN: usize = 1024;
pub const MAX_PATTERN_LEN: usize = 64;
/// Transform length produced by [`PHASE1_FIXTURE_TEXT_LEN`].
///
/// This is evidence metadata, not a runtime capacity limit.
pub const PHASE1_FIXTURE_TRANSFORM_LEN: usize = 2048;
pub const RFPS_RANDOMIZER_MIN: u8 = 1;
pub const RFPS_RANDOMIZER_MAX: u8 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    EmptyPattern,
    PatternLongerThanText {
        text_len: usize,
        pattern_len: usize,
    },
    TextLengthOverflow {
        actual: usize,
    },
    PatternLengthExceeded {
        actual: usize,
        maximum: usize,
    },
    RandomizerLengthMismatch {
        pattern_len: usize,
        randomizer_len: usize,
    },
    RandomizerOutsideSupportedDomain {
        index: usize,
        value: u8,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FpsTimeLayout {
    pub transform_len: usize,
    pub text: Vec<f64>,
    pub prefix_squares: Vec<f64>,
    pub reversed_pattern: Vec<f64>,
    pub pattern_square_sum: f64,
    pub valid_coefficients: RangeInclusive<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RfpsTimeLayout {
    pub transform_len: usize,
    pub text: Vec<f64>,
    pub reversed_randomizer: Vec<f64>,
    pub pattern_randomizer_dot_product: f64,
    pub valid_coefficients: RangeInclusive<usize>,
}

pub(crate) fn validated_transform_len_for_lengths(
    text_len: usize,
    pattern: &[u8],
) -> Result<usize, SearchError> {
    if pattern.is_empty() {
        return Err(SearchError::EmptyPattern);
    }
    if pattern.len() > MAX_PATTERN_LEN {
        return Err(SearchError::PatternLengthExceeded {
            actual: pattern.len(),
            maximum: MAX_PATTERN_LEN,
        });
    }
    if pattern.len() > text_len {
        return Err(SearchError::PatternLongerThanText {
            text_len,
            pattern_len: pattern.len(),
        });
    }
    let padded_len = text_len
        .checked_add(1)
        .ok_or(SearchError::TextLengthOverflow { actual: text_len })?;
    let transform_len = padded_len
        .checked_next_power_of_two()
        .ok_or(SearchError::TextLengthOverflow { actual: text_len })?;
    Ok(transform_len)
}

fn validated_transform_len(text: &[u8], pattern: &[u8]) -> Result<usize, SearchError> {
    validated_transform_len_for_lengths(text.len(), pattern)
}

/// Builds Feer's padded FPS time-domain vectors and valid coefficient range.
///
/// # Errors
///
/// Returns [`SearchError`] when pattern/text lengths violate the declared
/// prototype domain.
pub fn fps_time_layout(text: &[u8], pattern: &[u8]) -> Result<FpsTimeLayout, SearchError> {
    let transform_len = validated_transform_len(text, pattern)?;
    let mut padded_text = vec![0.0; transform_len];
    let mut prefix_squares = vec![0.0; transform_len];
    let mut running_square_sum = 0.0;
    for (index, value) in text.iter().copied().enumerate() {
        let value = f64::from(value);
        padded_text[index] = value;
        running_square_sum += value * value;
        prefix_squares[index] = running_square_sum;
    }

    let mut reversed_pattern = vec![0.0; transform_len];
    for (destination, value) in pattern.iter().rev().copied().enumerate() {
        reversed_pattern[destination] = f64::from(value);
    }
    let pattern_square_sum = pattern
        .iter()
        .copied()
        .map(f64::from)
        .map(|value| value * value)
        .sum();

    Ok(FpsTimeLayout {
        transform_len,
        text: padded_text,
        prefix_squares,
        reversed_pattern,
        pattern_square_sum,
        valid_coefficients: pattern.len() - 1..=text.len() - 1,
    })
}

/// Builds Feer's padded RFPS time-domain vectors and valid coefficient range.
///
/// # Errors
///
/// Returns [`SearchError`] when lengths or randomizer values violate the
/// declared prototype domain.
pub fn rfps_time_layout(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
) -> Result<RfpsTimeLayout, SearchError> {
    let transform_len = validated_transform_len(text, pattern)?;
    if randomizer.len() != pattern.len() {
        return Err(SearchError::RandomizerLengthMismatch {
            pattern_len: pattern.len(),
            randomizer_len: randomizer.len(),
        });
    }
    for (index, value) in randomizer.iter().copied().enumerate() {
        if !(RFPS_RANDOMIZER_MIN..=RFPS_RANDOMIZER_MAX).contains(&value) {
            return Err(SearchError::RandomizerOutsideSupportedDomain { index, value });
        }
    }

    let mut padded_text = vec![0.0; transform_len];
    for (destination, value) in text.iter().copied().enumerate() {
        padded_text[destination] = f64::from(value);
    }
    let mut reversed_randomizer = vec![0.0; transform_len];
    for (destination, value) in randomizer.iter().rev().copied().enumerate() {
        reversed_randomizer[destination] = f64::from(value);
    }
    let pattern_randomizer_dot_product = pattern
        .iter()
        .copied()
        .zip(randomizer.iter().copied())
        .map(|(pattern_value, randomizer_value)| {
            f64::from(pattern_value) * f64::from(randomizer_value)
        })
        .sum();

    Ok(RfpsTimeLayout {
        transform_len,
        text: padded_text,
        reversed_randomizer,
        pattern_randomizer_dot_product,
        valid_coefficients: pattern.len() - 1..=text.len() - 1,
    })
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn right_rotation_factors(transform_len: usize, amount: usize) -> Vec<Complex64> {
    (0..transform_len)
        .map(|frequency| {
            let angle = -TAU * (frequency * amount) as f64 / transform_len as f64;
            Complex64::from_polar(1.0, angle)
        })
        .collect()
}

pub(crate) fn complex_dft(values: &[f64]) -> Vec<Complex64> {
    dft(&values
        .iter()
        .copied()
        .map(|value| Complex64::new(value, 0.0))
        .collect::<Vec<_>>())
}

/// Evaluates Feer's FPS layout entirely in plaintext complex arithmetic.
///
/// # Errors
///
/// Returns [`SearchError`] when pattern/text lengths violate the declared
/// prototype domain.
pub fn feer_fps_scores(text: &[u8], pattern: &[u8]) -> Result<Vec<f64>, SearchError> {
    let layout = fps_time_layout(text, pattern)?;
    let text_spectrum = complex_dft(&layout.text);
    let prefix_spectrum = complex_dft(&layout.prefix_squares);
    let pattern_spectrum = complex_dft(&layout.reversed_pattern);
    let pattern_square_spectrum =
        complex_dft(&vec![layout.pattern_square_sum; layout.transform_len]);
    let rotation_factors = right_rotation_factors(layout.transform_len, pattern.len());

    let score_spectrum = (0..layout.transform_len)
        .map(|index| {
            prefix_spectrum[index] - prefix_spectrum[index] * rotation_factors[index]
                + pattern_square_spectrum[index]
                - 2.0 * text_spectrum[index] * pattern_spectrum[index]
        })
        .collect::<Vec<_>>();
    let score_vector = idft(&score_spectrum);
    Ok(layout
        .valid_coefficients
        .map(|index| score_vector[index].re)
        .collect())
}

/// Evaluates Feer's RFPS layout entirely in plaintext complex arithmetic.
///
/// # Errors
///
/// Returns [`SearchError`] when lengths or randomizer values violate the
/// declared prototype domain.
pub fn feer_rfps_scores(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
) -> Result<Vec<f64>, SearchError> {
    let layout = rfps_time_layout(text, pattern, randomizer)?;
    let text_spectrum = complex_dft(&layout.text);
    let randomizer_spectrum = complex_dft(&layout.reversed_randomizer);
    let pattern_randomizer_spectrum = complex_dft(&vec![
        layout.pattern_randomizer_dot_product;
        layout.transform_len
    ]);
    let score_spectrum = text_spectrum
        .iter()
        .zip(randomizer_spectrum)
        .zip(pattern_randomizer_spectrum)
        .map(|((text, randomizer), pattern_randomizer)| text * randomizer - pattern_randomizer)
        .collect::<Vec<_>>();
    let score_vector = idft(&score_spectrum);
    Ok(layout
        .valid_coefficients
        .map(|index| score_vector[index].re)
        .collect())
}
