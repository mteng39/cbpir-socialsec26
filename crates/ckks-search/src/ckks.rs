use num_complex::Complex64;
use openfhe::cxx::{CxxVector, UniquePtr};
use openfhe::ffi;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::classification::{ScoreDecision, classify_score};
use crate::layout::{SearchError, complex_dft};
use crate::{fps_time_layout, idft, rfps_time_layout, right_rotation_factors};

/// CKKS slot capacity used only to pack many independent small RFPS DFTs.
///
/// This is not the maximum transform length of one search.
pub const MAX_CKKS_BATCH_SIZE: usize = 4096;
const MAX_CKKS_MODULUS_BITS: u32 = 60;
/// Retained RFPS CKKS scaling-modulus size.
pub const RFPS_SCALING_MODULUS_BITS: u32 = 24;

/// CKKS level-management mode used by an FPS feasibility candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsScalingTechnique {
    FixedManual,
    FlexibleAutoExt,
}

impl FpsScalingTechnique {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FixedManual => "FIXEDMANUAL",
            Self::FlexibleAutoExt => "FLEXIBLEAUTOEXT",
        }
    }
}

/// Key-switch parameter used by an FPS feasibility candidate.
///
/// FPS server-owned-text Match performs no key switching. The field remains
/// explicit because OpenFHE includes HYBRID auxiliary moduli in its security
/// gate even when this prototype generates no evaluation keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsKeySwitchTechnique {
    Bv,
    Hybrid,
}

impl FpsKeySwitchTechnique {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bv => "BV",
            Self::Hybrid => "HYBRID",
        }
    }
}

/// Explicit CKKS parameters for controlled FPS feasibility experiments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FpsCkksParameters {
    pub ring_dimension: u32,
    pub batch_size: u32,
    pub multiplicative_depth: u32,
    pub first_modulus_bits: u32,
    pub scaling_modulus_bits: u32,
    pub scaling_technique: FpsScalingTechnique,
    pub key_switch_technique: FpsKeySwitchTechnique,
}

impl FpsCkksParameters {
    fn checked_layout(ring_dimension: u32, transform_len: usize) -> Result<u32, CkksError> {
        let batch_size = CkksConfig::checked_batch_size(transform_len)?;
        if ring_dimension == 0
            || !ring_dimension.is_power_of_two()
            || batch_size > ring_dimension / 2
        {
            return Err(CkksError::TransformExceedsProfileCapacity {
                transform_len,
                available_slots: (ring_dimension / 2) as usize,
            });
        }
        Ok(batch_size)
    }

    /// Reads the pinned OpenFHE CKKS defaults, then applies only the requested
    /// ring dimension, FPS depth, batch size, security level, and complex data
    /// type. This is the generator-led starting point for parameter repair.
    pub fn openfhe_recommended(
        ring_dimension: u32,
        transform_len: usize,
    ) -> Result<Self, CkksError> {
        let batch_size = Self::checked_layout(ring_dimension, transform_len)?;
        let parameters = ffi::GenParamsCKKSRNS();
        if parameters.GetScalingTechnique() != ffi::ScalingTechnique::FLEXIBLEAUTOEXT
            || parameters.GetKeySwitchTechnique() != ffi::KeySwitchTechnique::HYBRID
            || parameters.GetSecurityLevel() != ffi::SecurityLevel::HEStd_128_classic
            || parameters.GetSecretKeyDist() != ffi::SecretKeyDist::UNIFORM_TERNARY
            || (f64::from(parameters.GetStandardDeviation()) - 3.19).abs() > 1e-6
        {
            return Err(CkksError::UnexpectedOpenFheCkksDefaults);
        }
        Ok(Self {
            ring_dimension,
            batch_size,
            multiplicative_depth: 2,
            first_modulus_bits: parameters.GetFirstModSize(),
            scaling_modulus_bits: parameters.GetScalingModSize(),
            scaling_technique: FpsScalingTechnique::FlexibleAutoExt,
            key_switch_technique: FpsKeySwitchTechnique::Hybrid,
        })
    }

    /// Builds a controlled FIXEDMANUAL candidate without changing FPS work.
    pub fn fixed_manual(
        ring_dimension: u32,
        transform_len: usize,
        first_modulus_bits: u32,
        scaling_modulus_bits: u32,
        key_switch_technique: FpsKeySwitchTechnique,
    ) -> Result<Self, CkksError> {
        Ok(Self {
            ring_dimension,
            batch_size: Self::checked_layout(ring_dimension, transform_len)?,
            multiplicative_depth: 2,
            first_modulus_bits,
            scaling_modulus_bits,
            scaling_technique: FpsScalingTechnique::FixedManual,
            key_switch_technique,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CkksConfig {
    /// Explicit ring dimension for named feasibility profiles. `None` keeps
    /// the retained OpenFHE-selected default.
    pub ring_dimension: Option<u32>,
    pub batch_size: u32,
    pub multiplicative_depth: u32,
    pub scaling_modulus_bits: u32,
    pub first_modulus_bits: u32,
    pub decision_threshold: f64,
    pub scaling_technique: FpsScalingTechnique,
    pub key_switch_technique: FpsKeySwitchTechnique,
}

impl CkksConfig {
    fn checked_batch_size(transform_len: usize) -> Result<u32, CkksError> {
        if transform_len == 0 || !transform_len.is_power_of_two() {
            return Err(CkksError::UnsupportedTransformLength(transform_len));
        }
        u32::try_from(transform_len)
            .map_err(|_| CkksError::UnsupportedTransformLength(transform_len))
    }

    /// Builds the retained FPS CKKS configuration for a supported DFT length.
    ///
    /// # Errors
    ///
    /// Returns an error unless `transform_len` is a nonzero power of two that
    /// fits the retained FPS-14 profile's `N/2` CKKS slots.
    pub fn for_transform_len(transform_len: usize) -> Result<Self, CkksError> {
        Self::for_fps_transform_len(transform_len)
    }

    /// Builds the FPS translation of Feer's CKKS scale and threshold settings.
    ///
    /// # Errors
    ///
    /// Returns an error unless `transform_len` is a nonzero power of two that
    /// fits the retained FPS-14 profile's `N/2` CKKS slots.
    pub fn for_fps_transform_len(transform_len: usize) -> Result<Self, CkksError> {
        FpsProfile::Fps14.config_for_transform_len(transform_len)
    }

    /// Builds the RFPS translation of Feer's CKKS scale and threshold settings.
    ///
    /// # Errors
    ///
    /// Returns an error unless `transform_len` is a nonzero power of two that
    /// fits the retained RFPS-13 profile's `N/2` CKKS slots.
    pub fn for_rfps_transform_len(transform_len: usize) -> Result<Self, CkksError> {
        RfpsProfile::Rfps13.config_for_transform_len(transform_len)
    }

    /// Builds the retained RFPS configuration for a block-packed slot vector.
    ///
    /// # Errors
    ///
    /// Returns [`CkksError::UnsupportedBatchSize`] unless `batch_size` is a
    /// nonzero power of two no larger than [`MAX_CKKS_BATCH_SIZE`]. This check
    /// is intentionally distinct from the single-search transform-length cap.
    pub fn for_packed_rfps_batch_size(batch_size: usize) -> Result<Self, CkksError> {
        if batch_size == 0 || batch_size > MAX_CKKS_BATCH_SIZE || !batch_size.is_power_of_two() {
            return Err(CkksError::UnsupportedBatchSize(batch_size));
        }
        Ok(Self {
            ring_dimension: None,
            batch_size: u32::try_from(batch_size)
                .map_err(|_| CkksError::UnsupportedBatchSize(batch_size))?,
            multiplicative_depth: 1,
            scaling_modulus_bits: RFPS_SCALING_MODULUS_BITS,
            first_modulus_bits: 60,
            decision_threshold: 4.0,
            scaling_technique: FpsScalingTechnique::FixedManual,
            key_switch_technique: FpsKeySwitchTechnique::Hybrid,
        })
    }
}

macro_rules! ckks_profiles {
    ($name:ident, $prefix:literal, $depth:literal, $scale:expr, $($variant:ident => $log_n:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[must_use]
            pub const fn log_n(self) -> u32 {
                match self { $(Self::$variant => $log_n),+ }
            }

            #[must_use]
            pub const fn ring_dimension(self) -> u32 {
                1_u32 << self.log_n()
            }

            #[must_use]
            pub const fn label(self) -> &'static str {
                match self { $(Self::$variant => concat!($prefix, "-", stringify!($log_n))),+ }
            }

            /// Builds this named profile for one retained DFT layout.
            pub fn config_for_transform_len(self, transform_len: usize) -> Result<CkksConfig, CkksError> {
                let batch_size = CkksConfig::checked_batch_size(transform_len)?;
                if batch_size > self.ring_dimension() / 2 {
                    return Err(CkksError::TransformExceedsProfileCapacity {
                        transform_len,
                        available_slots: (self.ring_dimension() / 2) as usize,
                    });
                }
                Ok(CkksConfig {
                    ring_dimension: Some(self.ring_dimension()),
                    batch_size,
                    multiplicative_depth: $depth,
                    scaling_modulus_bits: $scale,
                    first_modulus_bits: 60,
                    decision_threshold: if $depth == 2 { 0.5 } else { 4.0 },
                    scaling_technique: FpsScalingTechnique::FixedManual,
                    key_switch_technique: FpsKeySwitchTechnique::Hybrid,
                })
            }
        }
    };
}

ckks_profiles!(RfpsProfile, "RFPS", 1, RFPS_SCALING_MODULUS_BITS, Rfps13 => 13, Rfps14 => 14, Rfps15 => 15);

/// Repaired FPS feasibility profiles. FPS-14 remains the retained reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsProfile {
    Fps13,
    Fps14,
    Fps15,
}

impl FpsProfile {
    #[must_use]
    pub const fn log_n(self) -> u32 {
        match self {
            Self::Fps13 => 13,
            Self::Fps14 => 14,
            Self::Fps15 => 15,
        }
    }

    #[must_use]
    pub const fn ring_dimension(self) -> u32 {
        1_u32 << self.log_n()
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fps13 => "FPS-13",
            Self::Fps14 => "FPS-14",
            Self::Fps15 => "FPS-15",
        }
    }

    #[must_use]
    pub const fn scaling_modulus_bits(self) -> u32 {
        match self {
            Self::Fps13 | Self::Fps15 => 50,
            Self::Fps14 => 40,
        }
    }

    #[must_use]
    pub const fn key_switch_technique(self) -> FpsKeySwitchTechnique {
        match self {
            Self::Fps13 => FpsKeySwitchTechnique::Bv,
            Self::Fps14 | Self::Fps15 => FpsKeySwitchTechnique::Hybrid,
        }
    }

    /// Builds the repaired explicit FPS parameters for one retained layout.
    pub fn parameters_for_transform_len(
        self,
        transform_len: usize,
    ) -> Result<FpsCkksParameters, CkksError> {
        FpsCkksParameters::fixed_manual(
            self.ring_dimension(),
            transform_len,
            60,
            self.scaling_modulus_bits(),
            self.key_switch_technique(),
        )
    }

    /// Complete context configuration for this repaired named profile.
    pub fn config_for_transform_len(self, transform_len: usize) -> Result<CkksConfig, CkksError> {
        let parameters = self.parameters_for_transform_len(transform_len)?;
        Ok(CkksConfig {
            ring_dimension: Some(parameters.ring_dimension),
            batch_size: parameters.batch_size,
            multiplicative_depth: parameters.multiplicative_depth,
            scaling_modulus_bits: parameters.scaling_modulus_bits,
            first_modulus_bits: parameters.first_modulus_bits,
            decision_threshold: 0.5,
            scaling_technique: parameters.scaling_technique,
            key_switch_technique: parameters.key_switch_technique,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CkksError {
    UnexpectedOpenFheCkksDefaults,
    UnsupportedTransformLength(usize),
    UnsupportedBatchSize(usize),
    TransformExceedsProfileCapacity {
        transform_len: usize,
        available_slots: usize,
    },
    UnsupportedScalingModulusBits(u32),
    OperandLengthMismatch {
        left: usize,
        right: usize,
    },
    EmptyPackedTrials,
    DuplicatePackedTrialId(String),
    PackedTrialLengthMismatch {
        trial_index: usize,
        text: usize,
        pattern: usize,
        randomizer: usize,
    },
    MixedPackedTransformLengths {
        expected: usize,
        actual: usize,
        trial_index: usize,
    },
    PackedSlotCountOverflow,
    PackedSlotCapacityExceeded {
        required: usize,
        maximum: usize,
    },
    InvalidDecisionThreshold,
    IoFailure {
        operation: &'static str,
    },
    SerializationFailure {
        artifact: &'static str,
    },
    DeserializationFailure {
        artifact: &'static str,
    },
    Search(SearchError),
}

impl From<SearchError> for CkksError {
    fn from(error: SearchError) -> Self {
        Self::Search(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CiphertextState {
    pub level: usize,
    pub noise_scale_degree: usize,
    pub scaling_factor: f64,
    pub component_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolPhaseTimings {
    pub context_and_key_generation_ns: u64,
    pub client_setup_ns: u64,
    pub client_query_ns: u64,
    pub server_evaluation_ns: u64,
    pub client_decrypt_idft_ns: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CkksRunMetrics {
    pub openfhe_version: String,
    pub ring_dimension: u32,
    pub batch_size: u32,
    pub multiplicative_depth: u32,
    pub scaling_modulus_bits: u32,
    pub first_modulus_bits: u32,
    pub input_state: CiphertextState,
    pub output_state: CiphertextState,
    pub ciphertext_multiplications: usize,
    pub ciphertext_plaintext_multiplications: usize,
    pub ciphertext_constant_multiplications: usize,
    pub additions: usize,
    pub subtractions: usize,
    pub relinearizations: usize,
    pub explicit_rescaling_operations: usize,
    pub scaling_technique: String,
    /// `OpenFHE` 1.5.1 does not implement this diagnostic for COMPLEX CKKS data.
    pub reported_log_precision_bits: Option<f64>,
    /// Protocol timings are present for FPS/RFPS runs and absent for the
    /// isolated componentwise-multiplication capability check.
    pub phase_timings: Option<ProtocolPhaseTimings>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CkksVectorResult {
    pub values: Vec<Complex64>,
    pub metrics: CkksRunMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncryptedFpsResult {
    pub scores: Vec<f64>,
    pub exact_scores: Vec<f64>,
    pub matching_positions: Vec<usize>,
    pub max_abs_error: f64,
    pub metrics: CkksRunMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncryptedRfpsResult {
    pub scores: Vec<f64>,
    pub exact_scores: Vec<i64>,
    pub matching_positions: Vec<usize>,
    pub decisions: Vec<ScoreDecision>,
    pub max_abs_error: f64,
    pub metrics: CkksRunMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfpsPackedTrialInput {
    pub id: String,
    pub text: Vec<u8>,
    pub pattern: Vec<u8>,
    pub randomizer: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RfpsPackedTrialResult {
    pub id: String,
    pub exact_score: i64,
    pub approximate_score: f64,
    pub decision: ScoreDecision,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncryptedPackedRfpsResult {
    pub trials: Vec<RfpsPackedTrialResult>,
    pub block_transform_len: usize,
    pub encoded_slot_count: usize,
    pub padding_slot_count: usize,
    pub padded_slot_count: usize,
    pub max_abs_error: f64,
    pub metrics: CkksRunMetrics,
}

#[derive(Debug, Clone, Copy)]
struct OperationCounts {
    ciphertext_multiplications: usize,
    ciphertext_plaintext_multiplications: usize,
    ciphertext_constant_multiplications: usize,
    additions: usize,
    subtractions: usize,
    explicit_rescaling_operations: usize,
}

pub(crate) struct CkksSession {
    pub(crate) context: UniquePtr<ffi::CryptoContextDCRTPoly>,
    pub(crate) key_pair: UniquePtr<ffi::KeyPairDCRTPoly>,
    pub(crate) config: CkksConfig,
}

impl CkksSession {
    pub(crate) fn new(config: CkksConfig) -> Self {
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
            .SetScalingTechnique(match config.scaling_technique {
                FpsScalingTechnique::FixedManual => ffi::ScalingTechnique::FIXEDMANUAL,
                FpsScalingTechnique::FlexibleAutoExt => ffi::ScalingTechnique::FLEXIBLEAUTOEXT,
            });
        parameters
            .pin_mut()
            .SetKeySwitchTechnique(match config.key_switch_technique {
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
        let key_pair = context.KeyGen();
        // OpenFHE stores evaluation keys in a process-global map. Its
        // context-scoped serializer includes every key attached to an equal
        // cached context, so isolate this synchronous prototype run first.
        ffi::DCRTPolyClearEvalMultKeys();
        context.EvalMultKeyGen(&key_pair.GetPrivateKey());
        Self {
            context,
            key_pair,
            config,
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
        operations: OperationCounts,
        phase_timings: Option<ProtocolPhaseTimings>,
    ) -> CkksRunMetrics {
        CkksRunMetrics {
            openfhe_version: ffi::GetOpenFHEVersion().to_string_lossy().into_owned(),
            ring_dimension: self.context.GetRingDimension(),
            batch_size: self.config.batch_size,
            multiplicative_depth: self.config.multiplicative_depth,
            scaling_modulus_bits: self.config.scaling_modulus_bits,
            first_modulus_bits: self.config.first_modulus_bits,
            input_state: ciphertext_state(input),
            output_state: ciphertext_state(output),
            ciphertext_multiplications: operations.ciphertext_multiplications,
            ciphertext_plaintext_multiplications: operations.ciphertext_plaintext_multiplications,
            ciphertext_constant_multiplications: operations.ciphertext_constant_multiplications,
            additions: operations.additions,
            subtractions: operations.subtractions,
            relinearizations: operations.ciphertext_multiplications,
            explicit_rescaling_operations: operations.explicit_rescaling_operations,
            scaling_technique: "FIXEDMANUAL".to_owned(),
            reported_log_precision_bits: None,
            phase_timings,
        }
    }
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

pub(crate) fn as_cxx_complex(
    values: &[Complex64],
) -> openfhe::cxx::UniquePtr<CxxVector<ffi::ComplexPair>> {
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

/// Multiplies two equally sized complex vectors through encrypted CKKS slots.
///
/// # Errors
///
/// Returns [`CkksError::OperandLengthMismatch`] for unequal operands or
/// [`CkksError::UnsupportedTransformLength`] for an unsupported slot count.
pub fn ckks_componentwise_product(
    left: &[Complex64],
    right: &[Complex64],
) -> Result<CkksVectorResult, CkksError> {
    if left.len() != right.len() {
        return Err(CkksError::OperandLengthMismatch {
            left: left.len(),
            right: right.len(),
        });
    }
    let config = CkksConfig::for_transform_len(left.len())?;
    let session = CkksSession::new(config);
    let left_ciphertext = session.encrypt(left);
    let right_ciphertext = session.encrypt(right);
    let product = session
        .context
        .EvalMultAndRelinearize(&left_ciphertext, &right_ciphertext);
    let values = session.decrypt(&product, left.len());
    let metrics = session.metrics(
        &left_ciphertext,
        &product,
        OperationCounts {
            ciphertext_multiplications: 1,
            ciphertext_plaintext_multiplications: 0,
            ciphertext_constant_multiplications: 0,
            additions: 0,
            subtractions: 0,
            explicit_rescaling_operations: 0,
        },
        None,
    );
    Ok(CkksVectorResult { values, metrics })
}

pub(crate) fn exact_fps_scores(text: &[u8], pattern: &[u8]) -> Vec<f64> {
    text.windows(pattern.len())
        .map(|window| {
            window
                .iter()
                .copied()
                .zip(pattern.iter().copied())
                .map(|(text_value, pattern_value)| {
                    let difference = i32::from(text_value) - i32::from(pattern_value);
                    f64::from(difference * difference)
                })
                .sum()
        })
        .collect()
}

/// Evaluates Feer's FPS equation (4.6) with direct-complex CKKS packing.
///
/// # Errors
///
/// Returns [`CkksError::InvalidDecisionThreshold`] for a negative or non-finite
/// threshold, or [`CkksError::Search`] when the search layout is invalid.
pub fn ckks_fps(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
) -> Result<EncryptedFpsResult, CkksError> {
    if !decision_threshold.is_finite() || decision_threshold < 0.0 {
        return Err(CkksError::InvalidDecisionThreshold);
    }
    let layout = fps_time_layout(text, pattern)?;
    let mut config = CkksConfig::for_fps_transform_len(layout.transform_len)?;
    config.decision_threshold = decision_threshold;
    let context_start = Instant::now();
    let session = CkksSession::new(config);
    let context_and_key_generation_ns = duration_ns(context_start.elapsed());

    let setup_start = Instant::now();
    let text_spectrum = complex_dft(&layout.text);
    let prefix_spectrum = complex_dft(&layout.prefix_squares);
    let encrypted_text = session.encrypt(&text_spectrum);
    let encrypted_prefix = session.encrypt(&prefix_spectrum);
    let client_setup_ns = duration_ns(setup_start.elapsed());

    let query_start = Instant::now();
    let reversed_pattern_spectrum = complex_dft(&layout.reversed_pattern);
    let pattern_square_spectrum =
        complex_dft(&vec![layout.pattern_square_sum; layout.transform_len]);
    let encrypted_pattern = session.encrypt(&reversed_pattern_spectrum);
    let encrypted_pattern_square = session.encrypt(&pattern_square_spectrum);
    let client_query_ns = duration_ns(query_start.elapsed());

    let server_start = Instant::now();
    let rotation_factors = right_rotation_factors(layout.transform_len, pattern.len());
    let mut rotation_plaintext = session.plaintext(&rotation_factors);

    let rotated_prefix = session
        .context
        .EvalMultByCiphertextAndPlaintext(&encrypted_prefix, rotation_plaintext.pin_mut());
    let cross = session
        .context
        .EvalMultAndRelinearize(&encrypted_text, &encrypted_pattern);
    let doubled_cross = session.context.EvalAddByCiphertexts(&cross, &cross);
    let unscaled_additive_terms = session
        .context
        .EvalAddByCiphertexts(&encrypted_prefix, &encrypted_pattern_square);
    let additive_terms = session
        .context
        .EvalMultByCiphertextAndConst(&unscaled_additive_terms, 1.0);
    let without_rotated_prefix = session
        .context
        .EvalSubByCiphertexts(&additive_terms, &rotated_prefix);
    let unscaled_scores = session
        .context
        .EvalSubByCiphertexts(&without_rotated_prefix, &doubled_cross);
    let encrypted_scores = session.context.Rescale(&unscaled_scores);
    let server_evaluation_ns = duration_ns(server_start.elapsed());

    let client_start = Instant::now();
    let decoded_spectrum = session.decrypt(&encrypted_scores, layout.transform_len);
    let decoded_vector = idft(&decoded_spectrum);
    let scores = layout
        .valid_coefficients
        .clone()
        .map(|index| decoded_vector[index].re)
        .collect::<Vec<_>>();
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
    let client_decrypt_idft_ns = duration_ns(client_start.elapsed());
    let metrics = session.metrics(
        &encrypted_text,
        &encrypted_scores,
        OperationCounts {
            ciphertext_multiplications: 1,
            ciphertext_plaintext_multiplications: 1,
            ciphertext_constant_multiplications: 1,
            additions: 2,
            subtractions: 2,
            explicit_rescaling_operations: 1,
        },
        Some(ProtocolPhaseTimings {
            context_and_key_generation_ns,
            client_setup_ns,
            client_query_ns,
            server_evaluation_ns,
            client_decrypt_idft_ns,
        }),
    );

    Ok(EncryptedFpsResult {
        scores,
        exact_scores,
        matching_positions,
        max_abs_error,
        metrics,
    })
}

pub(crate) fn exact_rfps_scores(text: &[u8], pattern: &[u8], randomizer: &[u8]) -> Vec<i64> {
    text.windows(pattern.len())
        .map(|window| {
            window
                .iter()
                .copied()
                .zip(pattern.iter().copied())
                .zip(randomizer.iter().copied())
                .map(|((text_value, pattern_value), randomizer_value)| {
                    (i64::from(text_value) - i64::from(pattern_value)) * i64::from(randomizer_value)
                })
                .sum()
        })
        .collect()
}

fn classify_rfps_scores(
    text: &[u8],
    pattern: &[u8],
    scores: &[f64],
    exact_scores: &[i64],
    decision_threshold: f64,
) -> Vec<ScoreDecision> {
    scores
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
        .collect()
}

/// Evaluates Feer's RFPS equation (4.10) with direct-complex CKKS packing.
///
/// # Errors
///
/// Returns [`CkksError::InvalidDecisionThreshold`] for a negative or non-finite
/// threshold, or [`CkksError::Search`] when the search layout is invalid.
#[allow(clippy::cast_precision_loss)] // Declared RFPS score bound is below 2^53.
pub fn ckks_rfps(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
) -> Result<EncryptedRfpsResult, CkksError> {
    ckks_rfps_with_scaling_modulus_bits(
        text,
        pattern,
        randomizer,
        decision_threshold,
        RFPS_SCALING_MODULUS_BITS,
    )
}

/// Evaluates RFPS with an explicit CKKS scaling-modulus size for parameter
/// calibration. All other RFPS settings remain equal to the retained mapping.
///
/// This API exists so failed approximation gates and any parameter change can
/// be reproduced one variable at a time. It does not change the default
/// [`ckks_rfps`] mapping.
///
/// # Errors
///
/// Returns [`CkksError::UnsupportedScalingModulusBits`] for zero or more than
/// 60 bits, [`CkksError::InvalidDecisionThreshold`] for a negative or
/// non-finite threshold, or [`CkksError::Search`] for invalid search input.
#[allow(clippy::cast_precision_loss)] // Declared RFPS score bound is below 2^53.
pub fn ckks_rfps_with_scaling_modulus_bits(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
    scaling_modulus_bits: u32,
) -> Result<EncryptedRfpsResult, CkksError> {
    if scaling_modulus_bits == 0 || scaling_modulus_bits > MAX_CKKS_MODULUS_BITS {
        return Err(CkksError::UnsupportedScalingModulusBits(
            scaling_modulus_bits,
        ));
    }
    if !decision_threshold.is_finite() || decision_threshold < 0.0 {
        return Err(CkksError::InvalidDecisionThreshold);
    }
    let layout = rfps_time_layout(text, pattern, randomizer)?;
    let mut config = CkksConfig::for_rfps_transform_len(layout.transform_len)?;
    config.decision_threshold = decision_threshold;
    config.scaling_modulus_bits = scaling_modulus_bits;
    let context_start = Instant::now();
    let session = CkksSession::new(config);
    let context_and_key_generation_ns = duration_ns(context_start.elapsed());

    let setup_start = Instant::now();
    let text_spectrum = complex_dft(&layout.text);
    let encrypted_text = session.encrypt(&text_spectrum);
    let client_setup_ns = duration_ns(setup_start.elapsed());

    let query_start = Instant::now();
    let randomizer_spectrum = complex_dft(&layout.reversed_randomizer);
    let pattern_randomizer_spectrum = complex_dft(&vec![
        layout.pattern_randomizer_dot_product;
        layout.transform_len
    ]);
    let encrypted_randomizer = session.encrypt(&randomizer_spectrum);
    let encrypted_pattern_randomizer = session.encrypt(&pattern_randomizer_spectrum);
    let client_query_ns = duration_ns(query_start.elapsed());

    let server_start = Instant::now();
    let cross = session
        .context
        .EvalMultAndRelinearize(&encrypted_text, &encrypted_randomizer);
    let scaled_pattern_randomizer = session
        .context
        .EvalMultByCiphertextAndConst(&encrypted_pattern_randomizer, 1.0);
    let encrypted_scores = session
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    let server_evaluation_ns = duration_ns(server_start.elapsed());

    let client_start = Instant::now();
    let decoded_spectrum = session.decrypt(&encrypted_scores, layout.transform_len);
    let decoded_vector = idft(&decoded_spectrum);
    let scores = layout
        .valid_coefficients
        .clone()
        .map(|index| decoded_vector[index].re)
        .collect::<Vec<_>>();
    let exact_scores = exact_rfps_scores(text, pattern, randomizer);
    let max_abs_error = scores
        .iter()
        .zip(&exact_scores)
        .map(|(approximate, exact)| (approximate - *exact as f64).abs())
        .fold(0.0_f64, f64::max);
    let decisions = classify_rfps_scores(text, pattern, &scores, &exact_scores, decision_threshold);
    let matching_positions = decisions
        .iter()
        .filter_map(|decision| {
            (decision.approximate_score.abs() <= decision_threshold).then_some(decision.position)
        })
        .collect();
    let client_decrypt_idft_ns = duration_ns(client_start.elapsed());
    let metrics = session.metrics(
        &encrypted_text,
        &encrypted_scores,
        OperationCounts {
            ciphertext_multiplications: 1,
            ciphertext_plaintext_multiplications: 0,
            ciphertext_constant_multiplications: 1,
            additions: 0,
            subtractions: 1,
            explicit_rescaling_operations: 0,
        },
        Some(ProtocolPhaseTimings {
            context_and_key_generation_ns,
            client_setup_ns,
            client_query_ns,
            server_evaluation_ns,
            client_decrypt_idft_ns,
        }),
    );

    Ok(EncryptedRfpsResult {
        scores,
        exact_scores,
        matching_positions,
        decisions,
        max_abs_error,
        metrics,
    })
}

/// Evaluates many equal-block-length, one-alignment RFPS trials in one CKKS batch.
///
/// Each trial retains its own DFT/IDFT block. Concatenation changes only SIMD
/// slot placement; it does not turn the blocks into one larger mathematical
/// transform.
///
/// # Errors
///
/// Returns a typed [`CkksError`] for an invalid threshold, empty/duplicate
/// trials, unequal per-trial vectors, mixed DFT block lengths, invalid RFPS
/// inputs, or capacity above [`MAX_CKKS_BATCH_SIZE`].
#[allow(clippy::cast_precision_loss)] // Declared exact RFPS scores are below 2^53.
#[allow(clippy::too_many_lines)] // Keep validation and five measured protocol phases together.
pub fn ckks_rfps_packed_trials(
    trials: &[RfpsPackedTrialInput],
    decision_threshold: f64,
) -> Result<EncryptedPackedRfpsResult, CkksError> {
    if !decision_threshold.is_finite() || decision_threshold < 0.0 {
        return Err(CkksError::InvalidDecisionThreshold);
    }
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
    let context_start = Instant::now();
    let session = CkksSession::new(config);
    let context_and_key_generation_ns = duration_ns(context_start.elapsed());

    let setup_start = Instant::now();
    let mut text_spectra = vec![Complex64::new(0.0, 0.0); MAX_CKKS_BATCH_SIZE];
    for (trial_index, layout) in layouts.iter().enumerate() {
        let offset = trial_index * block_transform_len;
        let spectrum = complex_dft(&layout.text);
        text_spectra[offset..offset + block_transform_len].copy_from_slice(&spectrum);
    }
    let encrypted_text = session.encrypt(&text_spectra);
    let client_setup_ns = duration_ns(setup_start.elapsed());

    let query_start = Instant::now();
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
    let encrypted_randomizer = session.encrypt(&randomizer_spectra);
    let encrypted_pattern_randomizer = session.encrypt(&pattern_randomizer_spectra);
    let client_query_ns = duration_ns(query_start.elapsed());

    let server_start = Instant::now();
    let cross = session
        .context
        .EvalMultAndRelinearize(&encrypted_text, &encrypted_randomizer);
    let scaled_pattern_randomizer = session
        .context
        .EvalMultByCiphertextAndConst(&encrypted_pattern_randomizer, 1.0);
    let encrypted_scores = session
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    let server_evaluation_ns = duration_ns(server_start.elapsed());

    let client_start = Instant::now();
    let decoded_spectra = session.decrypt(&encrypted_scores, MAX_CKKS_BATCH_SIZE);
    let mut results = Vec::with_capacity(trials.len());
    let mut max_abs_error = 0.0_f64;
    for (trial_index, (trial, layout)) in trials.iter().zip(&layouts).enumerate() {
        let offset = trial_index * block_transform_len;
        let decoded_block = idft(&decoded_spectra[offset..offset + block_transform_len]);
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
    let client_decrypt_idft_ns = duration_ns(client_start.elapsed());
    let metrics = session.metrics(
        &encrypted_text,
        &encrypted_scores,
        OperationCounts {
            ciphertext_multiplications: 1,
            ciphertext_plaintext_multiplications: 0,
            ciphertext_constant_multiplications: 1,
            additions: 0,
            subtractions: 1,
            explicit_rescaling_operations: 0,
        },
        Some(ProtocolPhaseTimings {
            context_and_key_generation_ns,
            client_setup_ns,
            client_query_ns,
            server_evaluation_ns,
            client_decrypt_idft_ns,
        }),
    );

    Ok(EncryptedPackedRfpsResult {
        trials: results,
        block_transform_len,
        encoded_slot_count,
        padding_slot_count: MAX_CKKS_BATCH_SIZE - encoded_slot_count,
        padded_slot_count: MAX_CKKS_BATCH_SIZE,
        max_abs_error,
        metrics,
    })
}
