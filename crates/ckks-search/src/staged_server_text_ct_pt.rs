//! Staged server-owned-text Match API used by protocol-level composition.
//!
//! This module exposes the existing ct-pt computation at its formal message
//! boundaries. It does not add cryptographic operations or change scores.

use num_complex::Complex64;
use openfhe::cxx::UniquePtr;
use openfhe::ffi;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::layout::{
    RFPS_RANDOMIZER_MAX, RFPS_RANDOMIZER_MIN, complex_dft, validated_transform_len_for_lengths,
};
use crate::server_text_ct_pt::{ServerTextCtPtSession, serialize_ciphertext};
use crate::{
    CkksConfig, CkksError, FpsCkksParameters, FpsProfile, RfpsProfile, SearchError, idft,
    right_rotation_factors,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerTextCtPtRecoveredScores {
    pub scores: Vec<f64>,
    pub matching_positions: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerTextCtPtMessageSizes {
    pub encrypted_query_ciphertext_bytes: Vec<u64>,
    pub encrypted_response_ciphertext_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerTextCtPtRuntimeProfile {
    pub security: &'static str,
    pub ring_dimension: u32,
    pub batch_size: u32,
    pub multiplicative_depth: u32,
    pub first_modulus_bits: u32,
    pub scaling_modulus_bits: u32,
    pub scaling_technique: &'static str,
    pub key_switch_technique: &'static str,
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
}

pub struct ServerTextCtPtFpsServerState {
    text_plaintext: UniquePtr<ffi::Plaintext>,
    prefix_spectrum: Vec<Complex64>,
    text_len: usize,
}

pub struct ServerTextCtPtRfpsServerState {
    text_plaintext: UniquePtr<ffi::Plaintext>,
    text_len: usize,
}

pub struct ServerTextCtPtFpsQuery {
    encrypted_pattern: UniquePtr<ffi::CiphertextDCRTPoly>,
    encrypted_pattern_square: UniquePtr<ffi::CiphertextDCRTPoly>,
    pattern_len: usize,
}

pub struct ServerTextCtPtRfpsQuery {
    encrypted_randomizer: UniquePtr<ffi::CiphertextDCRTPoly>,
    encrypted_pattern_randomizer: UniquePtr<ffi::CiphertextDCRTPoly>,
    pattern_len: usize,
}

pub struct ServerTextCtPtResponse {
    encrypted_scores: UniquePtr<ffi::CiphertextDCRTPoly>,
}

pub struct ServerTextCtPtFpsStaged {
    session: ServerTextCtPtSession,
    transform_len: usize,
}

pub struct ServerTextCtPtRfpsStaged {
    session: ServerTextCtPtSession,
    transform_len: usize,
}

fn validate_threshold(threshold: f64) -> Result<(), CkksError> {
    if threshold.is_finite() && threshold >= 0.0 {
        Ok(())
    } else {
        Err(CkksError::InvalidDecisionThreshold)
    }
}

fn validate_layout(text_len: usize, pattern: &[u8], transform_len: usize) -> Result<(), CkksError> {
    let actual = validated_transform_len_for_lengths(text_len, pattern)?;
    if actual == transform_len {
        Ok(())
    } else {
        Err(CkksError::UnsupportedTransformLength(actual))
    }
}

fn padded_text(text: &[u8], transform_len: usize) -> Result<Vec<f64>, CkksError> {
    let expected = text
        .len()
        .checked_add(1)
        .and_then(usize::checked_next_power_of_two)
        .ok_or(CkksError::Search(SearchError::TextLengthOverflow {
            actual: text.len(),
        }))?;
    if expected != transform_len {
        return Err(CkksError::UnsupportedTransformLength(expected));
    }
    let mut padded = vec![0.0; transform_len];
    for (destination, value) in text.iter().copied().enumerate() {
        padded[destination] = f64::from(value);
    }
    Ok(padded)
}

fn recovered_scores(
    session: &ServerTextCtPtSession,
    response: &ServerTextCtPtResponse,
    transform_len: usize,
    text_len: usize,
    pattern_len: usize,
    threshold: f64,
) -> Result<ServerTextCtPtRecoveredScores, CkksError> {
    validate_threshold(threshold)?;
    let scores = decrypt_idft_scores(session, response, transform_len, text_len, pattern_len)?;
    let matching_positions = scores
        .iter()
        .enumerate()
        .filter_map(|(position, score)| (score.abs() <= threshold).then_some(position))
        .collect();
    Ok(ServerTextCtPtRecoveredScores {
        scores,
        matching_positions,
    })
}

fn decrypt_idft_scores(
    session: &ServerTextCtPtSession,
    response: &ServerTextCtPtResponse,
    transform_len: usize,
    text_len: usize,
    pattern_len: usize,
) -> Result<Vec<f64>, CkksError> {
    let placeholder = vec![0_u8; pattern_len];
    validate_layout(text_len, &placeholder, transform_len)?;
    let decoded_spectrum = session.decrypt(&response.encrypted_scores, transform_len);
    let decoded = idft(&decoded_spectrum);
    Ok((pattern_len - 1..=text_len - 1)
        .map(|index| decoded[index].re)
        .collect())
}

fn runtime_profile(session: &ServerTextCtPtSession) -> ServerTextCtPtRuntimeProfile {
    ServerTextCtPtRuntimeProfile {
        security: "HEStd_128_classic",
        ring_dimension: session.context.GetRingDimension(),
        batch_size: session.config.batch_size,
        multiplicative_depth: session.config.multiplicative_depth,
        first_modulus_bits: session.config.first_modulus_bits,
        scaling_modulus_bits: session.config.scaling_modulus_bits,
        scaling_technique: session.scaling_technique.label(),
        key_switch_technique: session.key_switch_technique.label(),
        context_generation_ns: session.context_generation_ns,
        key_generation_ns: session.key_generation_ns,
    }
}

fn serialize_messages(
    directory: &Path,
    query: [&ffi::CiphertextDCRTPoly; 2],
    response: &ServerTextCtPtResponse,
) -> Result<ServerTextCtPtMessageSizes, CkksError> {
    fs::create_dir_all(directory).map_err(|_| CkksError::IoFailure {
        operation: "create staged message directory",
    })?;
    let encrypted_query_ciphertext_bytes = query
        .into_iter()
        .enumerate()
        .map(|(index, ciphertext)| {
            serialize_ciphertext(
                &directory.join(format!("query-{index}.bin")),
                ciphertext,
                "staged encrypted query",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let encrypted_response_ciphertext_bytes = serialize_ciphertext(
        &directory.join("response.bin"),
        &response.encrypted_scores,
        "staged encrypted response",
    )?;
    Ok(ServerTextCtPtMessageSizes {
        encrypted_query_ciphertext_bytes,
        encrypted_response_ciphertext_bytes,
    })
}

impl ServerTextCtPtFpsStaged {
    pub fn new(
        profile: FpsProfile,
        transform_len: usize,
        threshold: f64,
    ) -> Result<Self, CkksError> {
        let parameters = profile.parameters_for_transform_len(transform_len)?;
        Self::from_parameters(parameters, transform_len, threshold)
    }

    /// Builds a staged FPS context from an explicit, validated parameter set.
    pub fn from_parameters(
        parameters: FpsCkksParameters,
        transform_len: usize,
        threshold: f64,
    ) -> Result<Self, CkksError> {
        validate_threshold(threshold)?;
        if parameters.batch_size as usize != transform_len
            || transform_len == 0
            || !transform_len.is_power_of_two()
            || parameters.batch_size > parameters.ring_dimension / 2
        {
            return Err(CkksError::UnsupportedTransformLength(transform_len));
        }
        Ok(Self {
            session: ServerTextCtPtSession::new_for_fps(parameters, threshold),
            transform_len,
        })
    }

    #[must_use]
    pub fn runtime_profile(&self) -> ServerTextCtPtRuntimeProfile {
        runtime_profile(&self.session)
    }

    pub fn prepare_server(&self, text: &[u8]) -> Result<ServerTextCtPtFpsServerState, CkksError> {
        let text_values = padded_text(text, self.transform_len)?;
        let mut prefix_squares = vec![0.0; self.transform_len];
        let mut running = 0.0;
        for (index, value) in text.iter().copied().enumerate() {
            let value = f64::from(value);
            running += value * value;
            prefix_squares[index] = running;
        }
        let text_spectrum = complex_dft(&text_values);
        let prefix_spectrum = complex_dft(&prefix_squares);
        Ok(ServerTextCtPtFpsServerState {
            text_plaintext: self.session.plaintext(&text_spectrum),
            prefix_spectrum,
            text_len: text.len(),
        })
    }

    pub fn match_query(
        &self,
        text_len: usize,
        pattern: &[u8],
    ) -> Result<ServerTextCtPtFpsQuery, CkksError> {
        validate_layout(text_len, pattern, self.transform_len)?;
        let mut reversed = vec![0.0; self.transform_len];
        for (destination, value) in pattern.iter().rev().copied().enumerate() {
            reversed[destination] = f64::from(value);
        }
        let square_sum = pattern
            .iter()
            .copied()
            .map(f64::from)
            .map(|value| value * value)
            .sum();
        Ok(ServerTextCtPtFpsQuery {
            encrypted_pattern: self.session.encrypt(&complex_dft(&reversed)),
            encrypted_pattern_square: self
                .session
                .encrypt(&complex_dft(&vec![square_sum; self.transform_len])),
            pattern_len: pattern.len(),
        })
    }

    pub fn match_response(
        &self,
        server: &mut ServerTextCtPtFpsServerState,
        query: &ServerTextCtPtFpsQuery,
    ) -> Result<ServerTextCtPtResponse, CkksError> {
        if server.text_len + 1 > self.transform_len {
            return Err(CkksError::UnsupportedTransformLength(self.transform_len));
        }
        let rotation_factors = right_rotation_factors(self.transform_len, query.pattern_len);
        let prefix_delta = server
            .prefix_spectrum
            .iter()
            .zip(rotation_factors)
            .map(|(prefix, rotation)| prefix - prefix * rotation)
            .collect::<Vec<_>>();
        let mut prefix_plaintext = self.session.plaintext(&prefix_delta);
        let cross = self.session.context.EvalMultByCiphertextAndPlaintext(
            &query.encrypted_pattern,
            server.text_plaintext.pin_mut(),
        );
        let doubled_cross = self.session.context.EvalAddByCiphertexts(&cross, &cross);
        let unscaled_additive = self.session.context.EvalAddByCiphertextAndPlaintext(
            &query.encrypted_pattern_square,
            prefix_plaintext.pin_mut(),
        );
        let additive = self
            .session
            .context
            .EvalMultByCiphertextAndConst(&unscaled_additive, 1.0);
        let unscaled = self
            .session
            .context
            .EvalSubByCiphertexts(&additive, &doubled_cross);
        Ok(ServerTextCtPtResponse {
            encrypted_scores: self.session.context.Rescale(&unscaled),
        })
    }

    pub fn recover_scores(
        &self,
        response: &ServerTextCtPtResponse,
        text_len: usize,
        pattern_len: usize,
        threshold: f64,
    ) -> Result<ServerTextCtPtRecoveredScores, CkksError> {
        recovered_scores(
            &self.session,
            response,
            self.transform_len,
            text_len,
            pattern_len,
            threshold,
        )
    }

    pub fn decrypt_idft_scores(
        &self,
        response: &ServerTextCtPtResponse,
        text_len: usize,
        pattern_len: usize,
    ) -> Result<Vec<f64>, CkksError> {
        decrypt_idft_scores(
            &self.session,
            response,
            self.transform_len,
            text_len,
            pattern_len,
        )
    }

    pub fn serialize_messages(
        &self,
        directory: &Path,
        query: &ServerTextCtPtFpsQuery,
        response: &ServerTextCtPtResponse,
    ) -> Result<ServerTextCtPtMessageSizes, CkksError> {
        serialize_messages(
            directory,
            [&query.encrypted_pattern, &query.encrypted_pattern_square],
            response,
        )
    }
}

impl ServerTextCtPtRfpsStaged {
    pub fn new(
        profile: RfpsProfile,
        transform_len: usize,
        threshold: f64,
    ) -> Result<Self, CkksError> {
        let config = profile.config_for_transform_len(transform_len)?;
        Self::from_config(config, transform_len, threshold)
    }

    /// Builds a staged RFPS context from an explicit, validated configuration.
    pub fn from_config(
        mut config: CkksConfig,
        transform_len: usize,
        threshold: f64,
    ) -> Result<Self, CkksError> {
        validate_threshold(threshold)?;
        if config.batch_size as usize != transform_len
            || transform_len == 0
            || !transform_len.is_power_of_two()
            || config
                .ring_dimension
                .is_some_and(|ring_dimension| config.batch_size > ring_dimension / 2)
        {
            return Err(CkksError::UnsupportedTransformLength(transform_len));
        }
        config.decision_threshold = threshold;
        Ok(Self {
            session: ServerTextCtPtSession::new(config),
            transform_len,
        })
    }

    #[must_use]
    pub fn runtime_profile(&self) -> ServerTextCtPtRuntimeProfile {
        runtime_profile(&self.session)
    }

    pub fn prepare_server(&self, text: &[u8]) -> Result<ServerTextCtPtRfpsServerState, CkksError> {
        let spectrum = complex_dft(&padded_text(text, self.transform_len)?);
        Ok(ServerTextCtPtRfpsServerState {
            text_plaintext: self.session.plaintext(&spectrum),
            text_len: text.len(),
        })
    }

    pub fn match_query(
        &self,
        text_len: usize,
        pattern: &[u8],
        randomizer: &[u8],
    ) -> Result<ServerTextCtPtRfpsQuery, CkksError> {
        validate_layout(text_len, pattern, self.transform_len)?;
        if randomizer.len() != pattern.len() {
            return Err(CkksError::Search(SearchError::RandomizerLengthMismatch {
                pattern_len: pattern.len(),
                randomizer_len: randomizer.len(),
            }));
        }
        for (index, value) in randomizer.iter().copied().enumerate() {
            if !(RFPS_RANDOMIZER_MIN..=RFPS_RANDOMIZER_MAX).contains(&value) {
                return Err(CkksError::Search(
                    SearchError::RandomizerOutsideSupportedDomain { index, value },
                ));
            }
        }
        let mut reversed = vec![0.0; self.transform_len];
        for (destination, value) in randomizer.iter().rev().copied().enumerate() {
            reversed[destination] = f64::from(value);
        }
        let dot_product = pattern
            .iter()
            .copied()
            .zip(randomizer.iter().copied())
            .map(|(pattern_value, randomizer_value)| {
                f64::from(pattern_value) * f64::from(randomizer_value)
            })
            .sum();
        Ok(ServerTextCtPtRfpsQuery {
            encrypted_randomizer: self.session.encrypt(&complex_dft(&reversed)),
            encrypted_pattern_randomizer: self
                .session
                .encrypt(&complex_dft(&vec![dot_product; self.transform_len])),
            pattern_len: pattern.len(),
        })
    }

    pub fn match_response(
        &self,
        server: &mut ServerTextCtPtRfpsServerState,
        query: &ServerTextCtPtRfpsQuery,
    ) -> Result<ServerTextCtPtResponse, CkksError> {
        if server.text_len + 1 > self.transform_len || query.pattern_len == 0 {
            return Err(CkksError::UnsupportedTransformLength(self.transform_len));
        }
        let cross = self.session.context.EvalMultByCiphertextAndPlaintext(
            &query.encrypted_randomizer,
            server.text_plaintext.pin_mut(),
        );
        let scaled = self
            .session
            .context
            .EvalMultByCiphertextAndConst(&query.encrypted_pattern_randomizer, 1.0);
        Ok(ServerTextCtPtResponse {
            encrypted_scores: self.session.context.EvalSubByCiphertexts(&cross, &scaled),
        })
    }

    pub fn recover_scores(
        &self,
        response: &ServerTextCtPtResponse,
        text_len: usize,
        pattern_len: usize,
        threshold: f64,
    ) -> Result<ServerTextCtPtRecoveredScores, CkksError> {
        recovered_scores(
            &self.session,
            response,
            self.transform_len,
            text_len,
            pattern_len,
            threshold,
        )
    }

    pub fn decrypt_idft_scores(
        &self,
        response: &ServerTextCtPtResponse,
        text_len: usize,
        pattern_len: usize,
    ) -> Result<Vec<f64>, CkksError> {
        decrypt_idft_scores(
            &self.session,
            response,
            self.transform_len,
            text_len,
            pattern_len,
        )
    }

    pub fn serialize_messages(
        &self,
        directory: &Path,
        query: &ServerTextCtPtRfpsQuery,
        response: &ServerTextCtPtResponse,
    ) -> Result<ServerTextCtPtMessageSizes, CkksError> {
        serialize_messages(
            directory,
            [
                &query.encrypted_randomizer,
                &query.encrypted_pattern_randomizer,
            ],
            response,
        )
    }
}
