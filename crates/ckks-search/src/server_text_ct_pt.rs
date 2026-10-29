//! CKKS Match variant with server-owned plaintext searchable text.
//!
//! This module is additive. The retained `ckks` module remains the frozen
//! ciphertext--ciphertext baseline. Here the server keeps its text-derived DFT
//! operands as CKKS plaintexts while every pattern-derived query operand stays
//! encrypted.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use num_complex::Complex64;
use openfhe::cxx::{CxxVector, UniquePtr, let_cxx_string};
use openfhe::ffi;
use serde::{Deserialize, Serialize};

use crate::ckks::{exact_fps_scores, exact_rfps_scores};
use crate::classification::{ScoreDecision, classify_score};
use crate::layout::complex_dft;
use crate::{
    CiphertextState, CkksConfig, CkksError, FpsCkksParameters, FpsKeySwitchTechnique, FpsProfile,
    FpsScalingTechnique, MAX_CKKS_BATCH_SIZE, RfpsPackedTrialInput, RfpsPackedTrialResult,
    RfpsProfile, fps_time_layout, idft, rfps_time_layout, right_rotation_factors,
};

/// Stable report label for the server-owned-text Match variant.
pub const SERVER_TEXT_CT_PT_BACKEND: &str = "server_text_ct_pt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerTextCtPtOperationCounts {
    pub ciphertext_multiplications: usize,
    pub ciphertext_plaintext_multiplications: usize,
    pub ciphertext_constant_multiplications: usize,
    pub additions: usize,
    pub subtractions: usize,
    pub relinearizations: usize,
    pub explicit_rescaling_operations: usize,
    pub evaluated_multiplicative_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independently audited CKKS key requirement must be explicit in evidence"
)]
pub struct ServerTextCtPtKeyRequirements {
    pub public_key: bool,
    pub secret_key: bool,
    pub evaluation_multiplication_key: bool,
    pub relinearization_key: bool,
    pub rotation_keys: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerTextCtPtTimings {
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
    pub server_text_dft_preparation_ns: u64,
    pub server_plaintext_encoding_ns: u64,
    pub server_query_plaintext_preparation_ns: u64,
    pub client_query_dft_preparation_ns: u64,
    pub client_query_encryption_ns: u64,
    pub server_homomorphic_evaluation_ns: u64,
    pub client_decrypt_ns: u64,
    pub client_idft_ns: u64,
    pub oracle_error_instrumentation_ns: u64,
}

impl ServerTextCtPtTimings {
    /// Query-dependent Match time. It excludes context/key generation,
    /// query-independent server text DFT preparation/encoding, and oracle
    /// instrumentation.
    #[must_use]
    pub fn derived_online_match_ns(self) -> u64 {
        self.client_query_dft_preparation_ns
            .saturating_add(self.client_query_encryption_ns)
            .saturating_add(self.server_query_plaintext_preparation_ns)
            .saturating_add(self.server_homomorphic_evaluation_ns)
            .saturating_add(self.client_decrypt_ns)
            .saturating_add(self.client_idft_ns)
    }

    /// Full measured run, including setup and correctness instrumentation.
    #[must_use]
    pub fn cold_end_to_end_ns(self) -> u64 {
        self.context_generation_ns
            .saturating_add(self.key_generation_ns)
            .saturating_add(self.server_text_dft_preparation_ns)
            .saturating_add(self.server_plaintext_encoding_ns)
            .saturating_add(self.derived_online_match_ns())
            .saturating_add(self.oracle_error_instrumentation_ns)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerTextCtPtStateSizes {
    /// Key-independent numerical complex DFT values, counted as two `f64`s.
    pub plaintext_search_state_bytes: u64,
    pub encoded_server_plaintext_count: usize,
    pub encrypted_query_ciphertext_count: usize,
    pub encrypted_response_ciphertext_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerTextCtPtArtifactSizes {
    pub context_bytes: u64,
    pub public_key_bytes: u64,
    pub secret_key_bytes: u64,
    /// `None`: this path neither generates nor requires this key.
    pub evaluation_multiplication_key_bytes: Option<u64>,
    /// `None`: this path performs no homomorphic rotations.
    pub rotation_key_bytes: Option<u64>,
    pub plaintext_search_state_bytes: u64,
    /// The pinned wrapper exposes no plaintext-object serializer.
    pub encoded_server_plaintext_bytes: Option<u64>,
    pub encrypted_query_ciphertext_bytes: Vec<u64>,
    pub encrypted_response_ciphertext_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerTextCtPtMetrics {
    pub backend: &'static str,
    pub openfhe_version: String,
    pub ring_dimension: u32,
    pub batch_size: u32,
    pub configured_multiplicative_depth: u32,
    pub scaling_modulus_bits: u32,
    pub first_modulus_bits: u32,
    pub input_state: CiphertextState,
    pub output_state: CiphertextState,
    pub operations: ServerTextCtPtOperationCounts,
    pub keys: ServerTextCtPtKeyRequirements,
    pub timings: ServerTextCtPtTimings,
    pub state_sizes: ServerTextCtPtStateSizes,
    pub scaling_technique: String,
    pub key_switch_technique: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerTextCtPtFpsResult {
    pub scores: Vec<f64>,
    pub exact_scores: Vec<f64>,
    pub matching_positions: Vec<usize>,
    pub max_abs_error: f64,
    pub metrics: ServerTextCtPtMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerTextCtPtRfpsResult {
    pub scores: Vec<f64>,
    pub exact_scores: Vec<i64>,
    pub matching_positions: Vec<usize>,
    pub decisions: Vec<ScoreDecision>,
    pub max_abs_error: f64,
    pub metrics: ServerTextCtPtMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerTextCtPtPackedRfpsResult {
    pub trials: Vec<RfpsPackedTrialResult>,
    pub block_transform_len: usize,
    pub encoded_slot_count: usize,
    pub padding_slot_count: usize,
    pub padded_slot_count: usize,
    pub max_abs_error: f64,
    pub metrics: ServerTextCtPtMetrics,
}

pub(crate) struct ServerTextCtPtSession {
    pub(crate) context: UniquePtr<ffi::CryptoContextDCRTPoly>,
    pub(crate) key_pair: UniquePtr<ffi::KeyPairDCRTPoly>,
    pub(crate) config: CkksConfig,
    pub(crate) context_generation_ns: u64,
    pub(crate) key_generation_ns: u64,
    pub(crate) scaling_technique: FpsScalingTechnique,
    pub(crate) key_switch_technique: FpsKeySwitchTechnique,
}

impl ServerTextCtPtSession {
    pub(crate) fn new(config: CkksConfig) -> Self {
        Self::new_with_techniques(
            config,
            config.scaling_technique,
            config.key_switch_technique,
        )
    }

    pub(crate) fn new_for_fps(parameters: FpsCkksParameters, decision_threshold: f64) -> Self {
        Self::new_with_techniques(
            CkksConfig {
                ring_dimension: Some(parameters.ring_dimension),
                batch_size: parameters.batch_size,
                multiplicative_depth: parameters.multiplicative_depth,
                scaling_modulus_bits: parameters.scaling_modulus_bits,
                first_modulus_bits: parameters.first_modulus_bits,
                decision_threshold,
                scaling_technique: parameters.scaling_technique,
                key_switch_technique: parameters.key_switch_technique,
            },
            parameters.scaling_technique,
            parameters.key_switch_technique,
        )
    }

    fn new_with_techniques(
        config: CkksConfig,
        scaling_technique: FpsScalingTechnique,
        key_switch_technique: FpsKeySwitchTechnique,
    ) -> Self {
        let context_start = Instant::now();
        let mut parameters = ffi::GenParamsCKKSRNS();
        parameters
            .pin_mut()
            .SetMultiplicativeDepth(config.multiplicative_depth);
        parameters
            .pin_mut()
            .SetScalingModSize(config.scaling_modulus_bits);
        parameters
            .pin_mut()
            .SetFirstModSize(config.first_modulus_bits);
        parameters.pin_mut().SetBatchSize(config.batch_size);
        if let Some(ring_dimension) = config.ring_dimension {
            parameters.pin_mut().SetRingDim(ring_dimension);
        }
        parameters
            .pin_mut()
            .SetScalingTechnique(match scaling_technique {
                FpsScalingTechnique::FixedManual => ffi::ScalingTechnique::FIXEDMANUAL,
                FpsScalingTechnique::FlexibleAutoExt => ffi::ScalingTechnique::FLEXIBLEAUTOEXT,
            });
        parameters
            .pin_mut()
            .SetKeySwitchTechnique(match key_switch_technique {
                FpsKeySwitchTechnique::Bv => ffi::KeySwitchTechnique::BV,
                FpsKeySwitchTechnique::Hybrid => ffi::KeySwitchTechnique::HYBRID,
            });
        parameters
            .pin_mut()
            .SetSecurityLevel(ffi::SecurityLevel::HEStd_128_classic);
        parameters
            .pin_mut()
            .SetCKKSDataType(ffi::CKKSDataType::COMPLEX);
        let context = ffi::DCRTPolyGenCryptoContextByParamsCKKSRNS(&parameters);
        context.EnableByFeature(ffi::PKESchemeFeature::PKE);
        context.EnableByFeature(ffi::PKESchemeFeature::KEYSWITCH);
        context.EnableByFeature(ffi::PKESchemeFeature::LEVELEDSHE);
        let context_generation_ns = duration_ns(context_start.elapsed());

        // The OpenFHE map is process-global. Clear inherited keys, then prove
        // this path works without generating multiplication/relinearization
        // keys.
        ffi::DCRTPolyClearEvalMultKeys();
        let key_start = Instant::now();
        let key_pair = context.KeyGen();
        let key_generation_ns = duration_ns(key_start.elapsed());

        Self {
            context,
            key_pair,
            config,
            context_generation_ns,
            key_generation_ns,
            scaling_technique,
            key_switch_technique,
        }
    }

    pub(crate) fn plaintext(&self, values: &[Complex64]) -> UniquePtr<ffi::Plaintext> {
        let values = as_cxx_complex(values);
        self.context.MakeCKKSPackedPlaintextByVectorOfComplex(
            &values,
            1,
            0,
            &ffi::DCRTPolyGenNullParams(),
            self.config.batch_size,
        )
    }

    pub(crate) fn encrypt(&self, values: &[Complex64]) -> UniquePtr<ffi::CiphertextDCRTPoly> {
        let plaintext = self.plaintext(values);
        self.context
            .EncryptByPublicKey(&self.key_pair.GetPublicKey(), &plaintext)
    }

    pub(crate) fn decrypt(
        &self,
        ciphertext: &ffi::CiphertextDCRTPoly,
        length: usize,
    ) -> Vec<Complex64> {
        let mut decoded = ffi::GenNullPlainText();
        self.context.DecryptByPrivateKeyAndCiphertext(
            &self.key_pair.GetPrivateKey(),
            ciphertext,
            decoded.pin_mut(),
        );
        decoded.SetLength(length);
        decoded
            .GetCopyOfCKKSPackedValue()
            .iter()
            .map(|value| Complex64::new(value.re, value.im))
            .collect()
    }

    fn metrics(
        &self,
        input: &ffi::CiphertextDCRTPoly,
        output: &ffi::CiphertextDCRTPoly,
        operations: ServerTextCtPtOperationCounts,
        timings: ServerTextCtPtTimings,
        state_sizes: ServerTextCtPtStateSizes,
    ) -> ServerTextCtPtMetrics {
        ServerTextCtPtMetrics {
            backend: SERVER_TEXT_CT_PT_BACKEND,
            openfhe_version: ffi::GetOpenFHEVersion().to_string_lossy().into_owned(),
            ring_dimension: self.context.GetRingDimension(),
            batch_size: self.config.batch_size,
            configured_multiplicative_depth: self.config.multiplicative_depth,
            scaling_modulus_bits: self.config.scaling_modulus_bits,
            first_modulus_bits: self.config.first_modulus_bits,
            input_state: ciphertext_state(input),
            output_state: ciphertext_state(output),
            operations,
            keys: ServerTextCtPtKeyRequirements {
                public_key: true,
                secret_key: true,
                evaluation_multiplication_key: false,
                relinearization_key: false,
                rotation_keys: false,
            },
            timings,
            state_sizes,
            scaling_technique: self.scaling_technique.label().to_owned(),
            key_switch_technique: self.key_switch_technique.label().to_owned(),
        }
    }
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn as_cxx_complex(values: &[Complex64]) -> UniquePtr<CxxVector<ffi::ComplexPair>> {
    let mut result = CxxVector::<ffi::ComplexPair>::new();
    for value in values {
        result.pin_mut().push(ffi::ComplexPair {
            re: value.re,
            im: value.im,
        });
    }
    result
}

fn ciphertext_state(ciphertext: &ffi::CiphertextDCRTPoly) -> CiphertextState {
    CiphertextState {
        level: ciphertext.GetLevel(),
        noise_scale_degree: ciphertext.GetNoiseScaleDeg(),
        scaling_factor: ciphertext.GetScalingFactor(),
        component_count: ciphertext.GetComponentCount(),
    }
}

fn numerical_dft_bytes(value_count: usize) -> u64 {
    u64::try_from(value_count)
        .unwrap_or(u64::MAX)
        .saturating_mul(16)
}

fn validate_threshold(decision_threshold: f64) -> Result<(), CkksError> {
    if decision_threshold.is_finite() && decision_threshold >= 0.0 {
        Ok(())
    } else {
        Err(CkksError::InvalidDecisionThreshold)
    }
}

struct ArtifactDirectory {
    path: PathBuf,
}

impl ArtifactDirectory {
    fn new(protocol: &str) -> Result<Self, CkksError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CkksError::IoFailure {
                operation: "read system clock",
            })?
            .as_nanos();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "p0c-server-text-ct-pt-{protocol}-sizes-{}-{nonce}",
                std::process::id()
            ));
        fs::create_dir_all(&path).map_err(|_| CkksError::IoFailure {
            operation: "create serialization directory",
        })?;
        Ok(Self { path })
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for ArtifactDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn file_size(path: &Path) -> Result<u64, CkksError> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|_| CkksError::IoFailure {
            operation: "read serialized artifact metadata",
        })
}

fn serialize_context(path: &Path, context: &ffi::CryptoContextDCRTPoly) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializeCryptoContextToFile(&path_cxx, context, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure {
            artifact: "server-text crypto context",
        });
    }
    file_size(Path::new(path.as_ref()))
}

fn serialize_context_json_string(
    directory: &ArtifactDirectory,
    session: &ServerTextCtPtSession,
) -> Result<String, CkksError> {
    let path = directory.file("context.json");
    let path_text = path.to_string_lossy();
    let_cxx_string!(path_cxx = path_text.as_ref());
    if !ffi::DCRTPolySerializeCryptoContextToFile(
        &path_cxx,
        &session.context,
        ffi::SerialMode::JSON,
    ) {
        return Err(CkksError::SerializationFailure {
            artifact: "crypto context JSON",
        });
    }
    fs::read_to_string(path).map_err(|_| CkksError::IoFailure {
        operation: "read crypto context JSON",
    })
}

/// Returns OpenFHE's generated FPS context archive for parameter auditing.
pub fn server_text_ct_pt_fps_context_json(
    profile: FpsProfile,
    transform_len: usize,
) -> Result<String, CkksError> {
    let session = ServerTextCtPtSession::new_for_fps(
        profile.parameters_for_transform_len(transform_len)?,
        0.5,
    );
    serialize_context_json_string(&ArtifactDirectory::new("fps-context")?, &session)
}

/// Returns OpenFHE's generated context archive for one FPS repair candidate.
pub fn server_text_ct_pt_fps_context_json_with_parameters(
    parameters: FpsCkksParameters,
) -> Result<String, CkksError> {
    let session = ServerTextCtPtSession::new_for_fps(parameters, 0.5);
    serialize_context_json_string(&ArtifactDirectory::new("fps-repair-context")?, &session)
}

/// Returns OpenFHE's generated RFPS context archive for parameter auditing.
pub fn server_text_ct_pt_rfps_context_json(
    profile: RfpsProfile,
    transform_len: usize,
) -> Result<String, CkksError> {
    let session = ServerTextCtPtSession::new(profile.config_for_transform_len(transform_len)?);
    serialize_context_json_string(&ArtifactDirectory::new("rfps-context")?, &session)
}

fn serialize_public_key(path: &Path, key: &ffi::PublicKeyDCRTPoly) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializePublicKeyToFile(&path_cxx, key, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure {
            artifact: "server-text public key",
        });
    }
    file_size(Path::new(path.as_ref()))
}

fn serialize_secret_key(path: &Path, key: &ffi::PrivateKeyDCRTPoly) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializePrivateKeyToFile(&path_cxx, key, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure {
            artifact: "server-text secret key",
        });
    }
    file_size(Path::new(path.as_ref()))
}

pub(crate) fn serialize_ciphertext(
    path: &Path,
    ciphertext: &ffi::CiphertextDCRTPoly,
    artifact: &'static str,
) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializeCiphertextToFile(&path_cxx, ciphertext, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure { artifact });
    }
    file_size(Path::new(path.as_ref()))
}

fn common_artifact_sizes(
    directory: &ArtifactDirectory,
    session: &ServerTextCtPtSession,
    query_ciphertexts: &[&ffi::CiphertextDCRTPoly],
    response: &ffi::CiphertextDCRTPoly,
    plaintext_search_state_bytes: u64,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    let context_bytes = serialize_context(&directory.file("context.bin"), &session.context)?;
    let public_key_bytes = serialize_public_key(
        &directory.file("public-key.bin"),
        &session.key_pair.GetPublicKey(),
    )?;
    let secret_key_bytes = serialize_secret_key(
        &directory.file("secret-key.bin"),
        &session.key_pair.GetPrivateKey(),
    )?;
    let encrypted_query_ciphertext_bytes = query_ciphertexts
        .iter()
        .enumerate()
        .map(|(index, ciphertext)| {
            serialize_ciphertext(
                &directory.file(&format!("query-{index}.bin")),
                ciphertext,
                "server-text encrypted query",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let encrypted_response_ciphertext_bytes = serialize_ciphertext(
        &directory.file("response.bin"),
        response,
        "server-text encrypted response",
    )?;
    Ok(ServerTextCtPtArtifactSizes {
        context_bytes,
        public_key_bytes,
        secret_key_bytes,
        evaluation_multiplication_key_bytes: None,
        rotation_key_bytes: None,
        plaintext_search_state_bytes,
        encoded_server_plaintext_bytes: None,
        encrypted_query_ciphertext_bytes,
        encrypted_response_ciphertext_bytes,
    })
}

/// Measures serialized FPS context/key/query/response sizes for this variant.
/// The operation is separate from online timing.
///
/// # Errors
///
/// Returns [`CkksError`] for invalid input or serialization failure.
pub fn server_text_ct_pt_fps_artifact_sizes(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    server_text_ct_pt_fps_artifact_sizes_with_config(text, pattern, decision_threshold, None)
}

/// Measures FPS artifacts under one explicit feasibility profile.
pub fn server_text_ct_pt_fps_artifact_sizes_with_profile(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
    profile: FpsProfile,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    server_text_ct_pt_fps_artifact_sizes_with_config(
        text,
        pattern,
        decision_threshold,
        Some(profile),
    )
}

fn server_text_ct_pt_fps_artifact_sizes_with_config(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
    profile: Option<FpsProfile>,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = fps_time_layout(text, pattern)?;
    let session = match profile {
        Some(profile) => ServerTextCtPtSession::new_for_fps(
            profile.parameters_for_transform_len(layout.transform_len)?,
            decision_threshold,
        ),
        None => {
            ServerTextCtPtSession::new(CkksConfig::for_fps_transform_len(layout.transform_len)?)
        }
    };
    let text_spectrum = complex_dft(&layout.text);
    let prefix_spectrum = complex_dft(&layout.prefix_squares);
    let mut text_plaintext = session.plaintext(&text_spectrum);
    let encrypted_pattern = session.encrypt(&complex_dft(&layout.reversed_pattern));
    let encrypted_pattern_square = session.encrypt(&complex_dft(&vec![
        layout.pattern_square_sum;
        layout.transform_len
    ]));
    let rotation_factors = right_rotation_factors(layout.transform_len, pattern.len());
    let prefix_delta_spectrum = prefix_spectrum
        .iter()
        .zip(rotation_factors)
        .map(|(prefix, rotation)| prefix - prefix * rotation)
        .collect::<Vec<_>>();
    let mut prefix_delta_plaintext = session.plaintext(&prefix_delta_spectrum);
    let cross = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_pattern, text_plaintext.pin_mut());
    let doubled_cross = session.context.EvalAddByCiphertexts(&cross, &cross);
    let unscaled_additive_terms = session.context.EvalAddByCiphertextAndPlaintext(
        &encrypted_pattern_square,
        prefix_delta_plaintext.pin_mut(),
    );
    let additive_terms = session
        .context
        .EvalMultByCiphertextAndConst(&unscaled_additive_terms, 1.0);
    let unscaled_scores = session
        .context
        .EvalSubByCiphertexts(&additive_terms, &doubled_cross);
    let encrypted_scores = session.context.Rescale(&unscaled_scores);
    common_artifact_sizes(
        &ArtifactDirectory::new("fps")?,
        &session,
        &[&encrypted_pattern, &encrypted_pattern_square],
        &encrypted_scores,
        numerical_dft_bytes(text_spectrum.len().saturating_add(prefix_spectrum.len())),
    )
}

/// Measures serialized RFPS context/key/query/response sizes for this variant.
/// The operation is separate from online timing.
///
/// # Errors
///
/// Returns [`CkksError`] for invalid input or serialization failure.
pub fn server_text_ct_pt_rfps_artifact_sizes(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    server_text_ct_pt_rfps_artifact_sizes_with_config(
        text,
        pattern,
        randomizer,
        decision_threshold,
        None,
    )
}

/// Measures RFPS artifacts under one explicit feasibility profile.
pub fn server_text_ct_pt_rfps_artifact_sizes_with_profile(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
    profile: RfpsProfile,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    server_text_ct_pt_rfps_artifact_sizes_with_config(
        text,
        pattern,
        randomizer,
        decision_threshold,
        Some(profile),
    )
}

fn server_text_ct_pt_rfps_artifact_sizes_with_config(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
    profile: Option<RfpsProfile>,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = rfps_time_layout(text, pattern, randomizer)?;
    let config = match profile {
        Some(profile) => profile.config_for_transform_len(layout.transform_len)?,
        None => CkksConfig::for_rfps_transform_len(layout.transform_len)?,
    };
    let session = ServerTextCtPtSession::new(config);
    let text_spectrum = complex_dft(&layout.text);
    let mut text_plaintext = session.plaintext(&text_spectrum);
    let encrypted_randomizer = session.encrypt(&complex_dft(&layout.reversed_randomizer));
    let encrypted_pattern_randomizer = session.encrypt(&complex_dft(&vec![
        layout.pattern_randomizer_dot_product;
        layout.transform_len
    ]));
    let cross = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_randomizer, text_plaintext.pin_mut());
    let scaled_pattern_randomizer = session
        .context
        .EvalMultByCiphertextAndConst(&encrypted_pattern_randomizer, 1.0);
    let encrypted_scores = session
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    common_artifact_sizes(
        &ArtifactDirectory::new("rfps")?,
        &session,
        &[&encrypted_randomizer, &encrypted_pattern_randomizer],
        &encrypted_scores,
        numerical_dft_bytes(text_spectrum.len()),
    )
}

/// Measures serialized packed-RFPS context/key/query/response sizes for this
/// variant. The operation is separate from online timing.
///
/// # Errors
///
/// Returns [`CkksError`] for invalid input or serialization failure.
#[allow(clippy::too_many_lines)]
pub fn server_text_ct_pt_rfps_packed_artifact_sizes(
    trials: &[RfpsPackedTrialInput],
    decision_threshold: f64,
) -> Result<ServerTextCtPtArtifactSizes, CkksError> {
    validate_threshold(decision_threshold)?;
    if trials.is_empty() {
        return Err(CkksError::EmptyPackedTrials);
    }
    let mut ids = HashSet::new();
    let mut layouts = Vec::with_capacity(trials.len());
    let mut block_transform_len = None;
    for (trial_index, trial) in trials.iter().enumerate() {
        if !ids.insert(trial.id.as_str()) {
            return Err(CkksError::DuplicatePackedTrialId(trial.id.clone()));
        }
        if trial.text.len() != trial.pattern.len() || trial.pattern.len() != trial.randomizer.len()
        {
            return Err(CkksError::PackedTrialLengthMismatch {
                trial_index,
                text: trial.text.len(),
                pattern: trial.pattern.len(),
                randomizer: trial.randomizer.len(),
            });
        }
        let layout = rfps_time_layout(&trial.text, &trial.pattern, &trial.randomizer)?;
        if let Some(expected) = block_transform_len {
            if layout.transform_len != expected {
                return Err(CkksError::MixedPackedTransformLengths {
                    expected,
                    actual: layout.transform_len,
                    trial_index,
                });
            }
        } else {
            block_transform_len = Some(layout.transform_len);
        }
        layouts.push(layout);
    }
    let block_transform_len = block_transform_len.ok_or(CkksError::EmptyPackedTrials)?;
    let encoded_slot_count = trials
        .len()
        .checked_mul(block_transform_len)
        .ok_or(CkksError::PackedSlotCountOverflow)?;
    if encoded_slot_count > MAX_CKKS_BATCH_SIZE {
        return Err(CkksError::PackedSlotCapacityExceeded {
            required: encoded_slot_count,
            maximum: MAX_CKKS_BATCH_SIZE,
        });
    }

    let session =
        ServerTextCtPtSession::new(CkksConfig::for_packed_rfps_batch_size(MAX_CKKS_BATCH_SIZE)?);
    let mut text_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    let mut randomizer_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    let mut pattern_randomizer_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    for (trial_index, layout) in layouts.iter().enumerate() {
        let offset = trial_index * block_transform_len;
        text_spectra[offset..offset + block_transform_len]
            .copy_from_slice(&complex_dft(&layout.text));
        randomizer_spectra[offset..offset + block_transform_len]
            .copy_from_slice(&complex_dft(&layout.reversed_randomizer));
        pattern_randomizer_spectra[offset..offset + block_transform_len].copy_from_slice(
            &complex_dft(&vec![
                layout.pattern_randomizer_dot_product;
                block_transform_len
            ]),
        );
    }
    let mut text_plaintext = session.plaintext(&text_spectra);
    let encrypted_randomizer = session.encrypt(&randomizer_spectra);
    let encrypted_pattern_randomizer = session.encrypt(&pattern_randomizer_spectra);
    let cross = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_randomizer, text_plaintext.pin_mut());
    let scaled_pattern_randomizer = session
        .context
        .EvalMultByCiphertextAndConst(&encrypted_pattern_randomizer, 1.0);
    let encrypted_scores = session
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    common_artifact_sizes(
        &ArtifactDirectory::new("packed-rfps")?,
        &session,
        &[&encrypted_randomizer, &encrypted_pattern_randomizer],
        &encrypted_scores,
        numerical_dft_bytes(text_spectra.len()),
    )
}

/// Runs FPS with server-owned plaintext text spectra and encrypted query terms.
///
/// The server evaluates exactly one ciphertext--plaintext product and no
/// ciphertext--ciphertext products.
///
/// # Errors
///
/// Returns [`CkksError`] for an invalid threshold or search layout.
#[allow(clippy::too_many_lines)]
pub fn server_text_ct_pt_fps(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
) -> Result<ServerTextCtPtFpsResult, CkksError> {
    server_text_ct_pt_fps_with_config(text, pattern, decision_threshold, None)
}

/// Runs FPS with one explicit feasibility profile.
pub fn server_text_ct_pt_fps_with_profile(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
    profile: FpsProfile,
) -> Result<ServerTextCtPtFpsResult, CkksError> {
    let layout = fps_time_layout(text, pattern)?;
    server_text_ct_pt_fps_with_parameters(
        text,
        pattern,
        decision_threshold,
        profile.parameters_for_transform_len(layout.transform_len)?,
    )
}

/// Runs the unchanged FPS path with one explicit CKKS parameter candidate.
pub fn server_text_ct_pt_fps_with_parameters(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
    parameters: FpsCkksParameters,
) -> Result<ServerTextCtPtFpsResult, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = fps_time_layout(text, pattern)?;
    if parameters.batch_size != layout.transform_len as u32 {
        return Err(CkksError::UnsupportedBatchSize(
            parameters.batch_size as usize,
        ));
    }
    server_text_ct_pt_fps_with_session(
        text,
        pattern,
        decision_threshold,
        layout,
        ServerTextCtPtSession::new_for_fps(parameters, decision_threshold),
    )
}

fn server_text_ct_pt_fps_with_config(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
    profile: Option<FpsProfile>,
) -> Result<ServerTextCtPtFpsResult, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = fps_time_layout(text, pattern)?;
    let mut config = match profile {
        Some(profile) => profile.config_for_transform_len(layout.transform_len)?,
        None => CkksConfig::for_fps_transform_len(layout.transform_len)?,
    };
    config.decision_threshold = decision_threshold;
    let session = ServerTextCtPtSession::new(config);
    server_text_ct_pt_fps_with_session(text, pattern, decision_threshold, layout, session)
}

#[allow(clippy::too_many_lines)]
fn server_text_ct_pt_fps_with_session(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
    layout: crate::FpsTimeLayout,
    session: ServerTextCtPtSession,
) -> Result<ServerTextCtPtFpsResult, CkksError> {
    let server_dft_start = Instant::now();
    let text_spectrum = complex_dft(&layout.text);
    let prefix_spectrum = complex_dft(&layout.prefix_squares);
    let server_text_dft_preparation_ns = duration_ns(server_dft_start.elapsed());

    let server_encoding_start = Instant::now();
    let mut text_plaintext = session.plaintext(&text_spectrum);
    let server_plaintext_encoding_ns = duration_ns(server_encoding_start.elapsed());

    let query_dft_start = Instant::now();
    let reversed_pattern_spectrum = complex_dft(&layout.reversed_pattern);
    let pattern_square_spectrum =
        complex_dft(&vec![layout.pattern_square_sum; layout.transform_len]);
    let client_query_dft_preparation_ns = duration_ns(query_dft_start.elapsed());

    let query_encryption_start = Instant::now();
    let encrypted_pattern = session.encrypt(&reversed_pattern_spectrum);
    let encrypted_pattern_square = session.encrypt(&pattern_square_spectrum);
    let client_query_encryption_ns = duration_ns(query_encryption_start.elapsed());

    let server_query_plaintext_start = Instant::now();
    let rotation_factors = right_rotation_factors(layout.transform_len, pattern.len());
    let prefix_delta_spectrum = prefix_spectrum
        .iter()
        .zip(rotation_factors)
        .map(|(prefix, rotation)| prefix - prefix * rotation)
        .collect::<Vec<_>>();
    let mut prefix_delta_plaintext = session.plaintext(&prefix_delta_spectrum);
    let server_query_plaintext_preparation_ns = duration_ns(server_query_plaintext_start.elapsed());

    let server_start = Instant::now();
    let cross = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_pattern, text_plaintext.pin_mut());
    let doubled_cross = session.context.EvalAddByCiphertexts(&cross, &cross);
    let unscaled_additive_terms = session.context.EvalAddByCiphertextAndPlaintext(
        &encrypted_pattern_square,
        prefix_delta_plaintext.pin_mut(),
    );
    let additive_terms = session
        .context
        .EvalMultByCiphertextAndConst(&unscaled_additive_terms, 1.0);
    let unscaled_scores = session
        .context
        .EvalSubByCiphertexts(&additive_terms, &doubled_cross);
    let encrypted_scores = session.context.Rescale(&unscaled_scores);
    let server_homomorphic_evaluation_ns = duration_ns(server_start.elapsed());

    let decrypt_start = Instant::now();
    let decoded_spectrum = session.decrypt(&encrypted_scores, layout.transform_len);
    let client_decrypt_ns = duration_ns(decrypt_start.elapsed());
    let idft_start = Instant::now();
    let decoded_vector = idft(&decoded_spectrum);
    let scores = layout
        .valid_coefficients
        .clone()
        .map(|index| decoded_vector[index].re)
        .collect::<Vec<_>>();
    let client_idft_ns = duration_ns(idft_start.elapsed());

    let instrumentation_start = Instant::now();
    let exact_scores = exact_fps_scores(text, pattern);
    let max_abs_error = scores
        .iter()
        .zip(&exact_scores)
        .map(|(approximate, exact)| (approximate - exact).abs())
        .fold(0.0_f64, f64::max);
    let matching_positions = scores
        .iter()
        .enumerate()
        .filter_map(|(index, score)| (score.abs() <= decision_threshold).then_some(index))
        .collect();
    let oracle_error_instrumentation_ns = duration_ns(instrumentation_start.elapsed());

    let timings = ServerTextCtPtTimings {
        context_generation_ns: session.context_generation_ns,
        key_generation_ns: session.key_generation_ns,
        server_text_dft_preparation_ns,
        server_plaintext_encoding_ns,
        server_query_plaintext_preparation_ns,
        client_query_dft_preparation_ns,
        client_query_encryption_ns,
        server_homomorphic_evaluation_ns,
        client_decrypt_ns,
        client_idft_ns,
        oracle_error_instrumentation_ns,
    };
    let metrics = session.metrics(
        &encrypted_pattern,
        &encrypted_scores,
        ServerTextCtPtOperationCounts {
            ciphertext_multiplications: 0,
            ciphertext_plaintext_multiplications: 1,
            ciphertext_constant_multiplications: 1,
            additions: 2,
            subtractions: 1,
            relinearizations: 0,
            explicit_rescaling_operations: 1,
            evaluated_multiplicative_depth: 1,
        },
        timings,
        ServerTextCtPtStateSizes {
            plaintext_search_state_bytes: numerical_dft_bytes(
                text_spectrum.len().saturating_add(prefix_spectrum.len()),
            ),
            encoded_server_plaintext_count: 1,
            encrypted_query_ciphertext_count: 2,
            encrypted_response_ciphertext_count: 1,
        },
    );

    Ok(ServerTextCtPtFpsResult {
        scores,
        exact_scores,
        matching_positions,
        max_abs_error,
        metrics,
    })
}

/// Runs RFPS with a server-owned plaintext text spectrum and encrypted query.
///
/// # Errors
///
/// Returns [`CkksError`] for an invalid threshold or search layout.
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::too_many_lines)]
pub fn server_text_ct_pt_rfps(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
) -> Result<ServerTextCtPtRfpsResult, CkksError> {
    server_text_ct_pt_rfps_with_config(text, pattern, randomizer, decision_threshold, None)
}

/// Runs RFPS with one explicit feasibility profile.
pub fn server_text_ct_pt_rfps_with_profile(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
    profile: RfpsProfile,
) -> Result<ServerTextCtPtRfpsResult, CkksError> {
    server_text_ct_pt_rfps_with_config(text, pattern, randomizer, decision_threshold, Some(profile))
}

fn server_text_ct_pt_rfps_with_config(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
    profile: Option<RfpsProfile>,
) -> Result<ServerTextCtPtRfpsResult, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = rfps_time_layout(text, pattern, randomizer)?;
    let mut config = match profile {
        Some(profile) => profile.config_for_transform_len(layout.transform_len)?,
        None => CkksConfig::for_rfps_transform_len(layout.transform_len)?,
    };
    config.decision_threshold = decision_threshold;
    let session = ServerTextCtPtSession::new(config);

    let server_dft_start = Instant::now();
    let text_spectrum = complex_dft(&layout.text);
    let server_text_dft_preparation_ns = duration_ns(server_dft_start.elapsed());
    let server_encoding_start = Instant::now();
    let mut text_plaintext = session.plaintext(&text_spectrum);
    let server_plaintext_encoding_ns = duration_ns(server_encoding_start.elapsed());

    let query_dft_start = Instant::now();
    let randomizer_spectrum = complex_dft(&layout.reversed_randomizer);
    let pattern_randomizer_spectrum = complex_dft(&vec![
        layout.pattern_randomizer_dot_product;
        layout.transform_len
    ]);
    let client_query_dft_preparation_ns = duration_ns(query_dft_start.elapsed());
    let query_encryption_start = Instant::now();
    let encrypted_randomizer = session.encrypt(&randomizer_spectrum);
    let encrypted_pattern_randomizer = session.encrypt(&pattern_randomizer_spectrum);
    let client_query_encryption_ns = duration_ns(query_encryption_start.elapsed());

    let server_query_plaintext_start = Instant::now();
    let server_query_plaintext_preparation_ns = duration_ns(server_query_plaintext_start.elapsed());
    let server_start = Instant::now();
    let cross = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_randomizer, text_plaintext.pin_mut());
    let scaled_pattern_randomizer = session
        .context
        .EvalMultByCiphertextAndConst(&encrypted_pattern_randomizer, 1.0);
    let encrypted_scores = session
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    let server_homomorphic_evaluation_ns = duration_ns(server_start.elapsed());

    let decrypt_start = Instant::now();
    let decoded_spectrum = session.decrypt(&encrypted_scores, layout.transform_len);
    let client_decrypt_ns = duration_ns(decrypt_start.elapsed());
    let idft_start = Instant::now();
    let decoded_vector = idft(&decoded_spectrum);
    let scores = layout
        .valid_coefficients
        .clone()
        .map(|index| decoded_vector[index].re)
        .collect::<Vec<_>>();
    let client_idft_ns = duration_ns(idft_start.elapsed());

    let instrumentation_start = Instant::now();
    let exact_scores = exact_rfps_scores(text, pattern, randomizer);
    let max_abs_error = scores
        .iter()
        .zip(&exact_scores)
        .map(|(approximate, exact)| (approximate - *exact as f64).abs())
        .fold(0.0_f64, f64::max);
    let decisions = scores
        .iter()
        .copied()
        .zip(exact_scores.iter().copied())
        .enumerate()
        .map(|(position, (approximate_score, exact_score))| {
            let window_matches = text[position..position + pattern.len()] == *pattern;
            ScoreDecision {
                position,
                exact_score,
                approximate_score,
                window_matches,
                cause: classify_score(
                    window_matches,
                    exact_score,
                    approximate_score,
                    decision_threshold,
                ),
            }
        })
        .collect::<Vec<_>>();
    let matching_positions = decisions
        .iter()
        .filter_map(|decision| {
            (decision.approximate_score.abs() <= decision_threshold).then_some(decision.position)
        })
        .collect();
    let oracle_error_instrumentation_ns = duration_ns(instrumentation_start.elapsed());

    let timings = ServerTextCtPtTimings {
        context_generation_ns: session.context_generation_ns,
        key_generation_ns: session.key_generation_ns,
        server_text_dft_preparation_ns,
        server_plaintext_encoding_ns,
        server_query_plaintext_preparation_ns,
        client_query_dft_preparation_ns,
        client_query_encryption_ns,
        server_homomorphic_evaluation_ns,
        client_decrypt_ns,
        client_idft_ns,
        oracle_error_instrumentation_ns,
    };
    let metrics = session.metrics(
        &encrypted_randomizer,
        &encrypted_scores,
        ServerTextCtPtOperationCounts {
            ciphertext_multiplications: 0,
            ciphertext_plaintext_multiplications: 1,
            ciphertext_constant_multiplications: 1,
            additions: 0,
            subtractions: 1,
            relinearizations: 0,
            explicit_rescaling_operations: 0,
            evaluated_multiplicative_depth: 1,
        },
        timings,
        ServerTextCtPtStateSizes {
            plaintext_search_state_bytes: numerical_dft_bytes(text_spectrum.len()),
            encoded_server_plaintext_count: 1,
            encrypted_query_ciphertext_count: 2,
            encrypted_response_ciphertext_count: 1,
        },
    );

    Ok(ServerTextCtPtRfpsResult {
        scores,
        exact_scores,
        matching_positions,
        decisions,
        max_abs_error,
        metrics,
    })
}

/// Evaluates equal-block-length RFPS trials with server text in CKKS
/// plaintext slots. Every block has its own DFT and IDFT.
///
/// # Errors
///
/// Returns [`CkksError`] for invalid thresholds, trial shapes, IDs, domains,
/// or packed capacity.
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::too_many_lines)]
pub fn server_text_ct_pt_rfps_packed_trials(
    trials: &[RfpsPackedTrialInput],
    decision_threshold: f64,
) -> Result<ServerTextCtPtPackedRfpsResult, CkksError> {
    validate_threshold(decision_threshold)?;
    if trials.is_empty() {
        return Err(CkksError::EmptyPackedTrials);
    }

    let mut ids = HashSet::new();
    let mut layouts = Vec::with_capacity(trials.len());
    let mut block_transform_len = None;
    for (trial_index, trial) in trials.iter().enumerate() {
        if !ids.insert(trial.id.as_str()) {
            return Err(CkksError::DuplicatePackedTrialId(trial.id.clone()));
        }
        if trial.text.len() != trial.pattern.len() || trial.pattern.len() != trial.randomizer.len()
        {
            return Err(CkksError::PackedTrialLengthMismatch {
                trial_index,
                text: trial.text.len(),
                pattern: trial.pattern.len(),
                randomizer: trial.randomizer.len(),
            });
        }
        let layout = rfps_time_layout(&trial.text, &trial.pattern, &trial.randomizer)?;
        if let Some(expected) = block_transform_len {
            if layout.transform_len != expected {
                return Err(CkksError::MixedPackedTransformLengths {
                    expected,
                    actual: layout.transform_len,
                    trial_index,
                });
            }
        } else {
            block_transform_len = Some(layout.transform_len);
        }
        layouts.push(layout);
    }
    let block_transform_len = block_transform_len.ok_or(CkksError::EmptyPackedTrials)?;
    let encoded_slot_count = trials
        .len()
        .checked_mul(block_transform_len)
        .ok_or(CkksError::PackedSlotCountOverflow)?;
    if encoded_slot_count > MAX_CKKS_BATCH_SIZE {
        return Err(CkksError::PackedSlotCapacityExceeded {
            required: encoded_slot_count,
            maximum: MAX_CKKS_BATCH_SIZE,
        });
    }

    let mut config = CkksConfig::for_packed_rfps_batch_size(MAX_CKKS_BATCH_SIZE)?;
    config.decision_threshold = decision_threshold;
    let session = ServerTextCtPtSession::new(config);

    let server_dft_start = Instant::now();
    let mut text_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    for (trial_index, layout) in layouts.iter().enumerate() {
        let offset = trial_index * block_transform_len;
        let spectrum = complex_dft(&layout.text);
        text_spectra[offset..offset + block_transform_len].copy_from_slice(&spectrum);
    }
    let server_text_dft_preparation_ns = duration_ns(server_dft_start.elapsed());
    let server_encoding_start = Instant::now();
    let mut text_plaintext = session.plaintext(&text_spectra);
    let server_plaintext_encoding_ns = duration_ns(server_encoding_start.elapsed());

    let query_dft_start = Instant::now();
    let mut randomizer_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    let mut pattern_randomizer_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    for (trial_index, layout) in layouts.iter().enumerate() {
        let offset = trial_index * block_transform_len;
        let randomizer_spectrum = complex_dft(&layout.reversed_randomizer);
        randomizer_spectra[offset..offset + block_transform_len]
            .copy_from_slice(&randomizer_spectrum);
        let pattern_randomizer_spectrum = complex_dft(&vec![
            layout.pattern_randomizer_dot_product;
            block_transform_len
        ]);
        pattern_randomizer_spectra[offset..offset + block_transform_len]
            .copy_from_slice(&pattern_randomizer_spectrum);
    }
    let client_query_dft_preparation_ns = duration_ns(query_dft_start.elapsed());
    let query_encryption_start = Instant::now();
    let encrypted_randomizer = session.encrypt(&randomizer_spectra);
    let encrypted_pattern_randomizer = session.encrypt(&pattern_randomizer_spectra);
    let client_query_encryption_ns = duration_ns(query_encryption_start.elapsed());

    let server_query_plaintext_start = Instant::now();
    let server_query_plaintext_preparation_ns = duration_ns(server_query_plaintext_start.elapsed());
    let server_start = Instant::now();
    let cross = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_randomizer, text_plaintext.pin_mut());
    let scaled_pattern_randomizer = session
        .context
        .EvalMultByCiphertextAndConst(&encrypted_pattern_randomizer, 1.0);
    let encrypted_scores = session
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    let server_homomorphic_evaluation_ns = duration_ns(server_start.elapsed());

    let decrypt_start = Instant::now();
    let decoded_spectra = session.decrypt(&encrypted_scores, MAX_CKKS_BATCH_SIZE);
    let client_decrypt_ns = duration_ns(decrypt_start.elapsed());
    let idft_start = Instant::now();
    let mut decoded_blocks = Vec::with_capacity(trials.len());
    for trial_index in 0..trials.len() {
        let offset = trial_index * block_transform_len;
        decoded_blocks.push(idft(&decoded_spectra[offset..offset + block_transform_len]));
    }
    let client_idft_ns = duration_ns(idft_start.elapsed());

    let instrumentation_start = Instant::now();
    let mut results = Vec::with_capacity(trials.len());
    let mut max_abs_error = 0.0_f64;
    for (trial_index, ((trial, layout), decoded_block)) in
        trials.iter().zip(&layouts).zip(&decoded_blocks).enumerate()
    {
        let _ = trial_index;
        let coefficient = *layout.valid_coefficients.start();
        let approximate_score = decoded_block[coefficient].re;
        let exact_score = exact_rfps_scores(&trial.text, &trial.pattern, &trial.randomizer)[0];
        max_abs_error = max_abs_error.max((approximate_score - exact_score as f64).abs());
        let window_matches = trial.text == trial.pattern;
        let decision = ScoreDecision {
            position: 0,
            exact_score,
            approximate_score,
            window_matches,
            cause: classify_score(
                window_matches,
                exact_score,
                approximate_score,
                decision_threshold,
            ),
        };
        results.push(RfpsPackedTrialResult {
            id: trial.id.clone(),
            exact_score,
            approximate_score,
            decision,
        });
    }
    let oracle_error_instrumentation_ns = duration_ns(instrumentation_start.elapsed());

    let timings = ServerTextCtPtTimings {
        context_generation_ns: session.context_generation_ns,
        key_generation_ns: session.key_generation_ns,
        server_text_dft_preparation_ns,
        server_plaintext_encoding_ns,
        server_query_plaintext_preparation_ns,
        client_query_dft_preparation_ns,
        client_query_encryption_ns,
        server_homomorphic_evaluation_ns,
        client_decrypt_ns,
        client_idft_ns,
        oracle_error_instrumentation_ns,
    };
    let metrics = session.metrics(
        &encrypted_randomizer,
        &encrypted_scores,
        ServerTextCtPtOperationCounts {
            ciphertext_multiplications: 0,
            ciphertext_plaintext_multiplications: 1,
            ciphertext_constant_multiplications: 1,
            additions: 0,
            subtractions: 1,
            relinearizations: 0,
            explicit_rescaling_operations: 0,
            evaluated_multiplicative_depth: 1,
        },
        timings,
        ServerTextCtPtStateSizes {
            plaintext_search_state_bytes: numerical_dft_bytes(text_spectra.len()),
            encoded_server_plaintext_count: 1,
            encrypted_query_ciphertext_count: 2,
            encrypted_response_ciphertext_count: 1,
        },
    );

    Ok(ServerTextCtPtPackedRfpsResult {
        trials: results,
        block_transform_len,
        encoded_slot_count,
        padding_slot_count: MAX_CKKS_BATCH_SIZE - encoded_slot_count,
        padded_slot_count: MAX_CKKS_BATCH_SIZE,
        max_abs_error,
        metrics,
    })
}
