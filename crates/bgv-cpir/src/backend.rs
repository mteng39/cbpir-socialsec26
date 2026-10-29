//! OpenFHE-backed cryptographic adapter for the audited P0-A fixed-window
//! index-CPIR protocol.

use openfhe::cxx::{CxxVector, UniquePtr};
use openfhe::ffi;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

/// Requested parameters and packing rules for the cryptographic kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BgvCpirConfig {
    /// Requested BGV polynomial/ring dimension.
    pub ring_dimension: usize,
    /// BGV plaintext modulus.
    pub plaintext_modulus: u64,
    /// Fixed record-window width in SIMD slots.
    pub record_slots: usize,
    /// Multiplicative depth requested from `OpenFHE`.
    pub multiplicative_depth: u32,
}

impl BgvCpirConfig {
    /// Prior Go/Lattigo mini-case packing with an OpenFHE-generated modulus chain.
    #[must_use]
    pub const fn prior_mini() -> Self {
        Self {
            ring_dimension: 8192,
            plaintext_modulus: 65_537,
            record_slots: 128,
            multiplicative_depth: 1,
        }
    }
}

/// Named BGV profiles used by the CBPIR feasibility gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BgvProfile {
    Bgv13,
    Bgv14,
    Bgv15,
}

impl BgvProfile {
    #[must_use]
    pub const fn log_n(self) -> u32 {
        match self {
            Self::Bgv13 => 13,
            Self::Bgv14 => 14,
            Self::Bgv15 => 15,
        }
    }

    #[must_use]
    pub const fn ring_dimension(self) -> usize {
        1_usize << self.log_n()
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bgv13 => "BGV-13",
            Self::Bgv14 => "BGV-14",
            Self::Bgv15 => "BGV-15",
        }
    }

    #[must_use]
    pub const fn config(self) -> BgvCpirConfig {
        BgvCpirConfig {
            ring_dimension: self.ring_dimension(),
            plaintext_modulus: 65_537,
            record_slots: 128,
            multiplicative_depth: 1,
        }
    }
}

/// Validation and decode failures handled before or after the native HE call.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum CpirError {
    /// A configuration field was zero or inconsistent.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(&'static str),
    /// Retained for source compatibility with older callers. Empty selectors
    /// now encode the valid all-zero selection.
    #[error("legacy empty-selection error")]
    EmptySelection,
    /// Retained for source compatibility with older exact-division packing checks.
    #[error("actual ring dimension {actual} is not divisible by record width {record_slots}")]
    IncompatibleRingDimension { actual: usize, record_slots: usize },
    /// A physical record index exceeds packed capacity.
    #[error("index {index} is outside physical capacity {capacity}")]
    IndexOutOfRange { index: usize, capacity: usize },
    /// A physical record index appears more than once in a binary selector.
    #[error("index {index} appears more than once in the selector")]
    DuplicateIndex { index: usize },
    /// No database records were supplied.
    #[error("database must contain at least one record")]
    EmptyDatabase,
    /// The database has more records than fit in the packed slot domain.
    #[error("database has {actual} records but capacity is {capacity}")]
    TooManyRecords { actual: usize, capacity: usize },
    /// A record exceeds the fixed packed window.
    #[error("record {index} has {actual} bytes but the window permits {maximum}")]
    RecordTooLong {
        index: usize,
        actual: usize,
        maximum: usize,
    },
    /// A decrypted response does not contain the expected complete slot vector.
    #[error("response has {actual} slots but {expected} were required")]
    WrongSlotCount { actual: usize, expected: usize },
    /// Requested fixed decode length exceeds one record window.
    #[error("decode length {actual} exceeds record window {maximum}")]
    DecodeLengthTooLong { actual: usize, maximum: usize },
    /// A decrypted plaintext slot cannot represent one byte.
    #[error("slot {position} has non-byte value {value}")]
    NonByteSlot { position: usize, value: u64 },
}

/// Native binary-serialization failures.
#[derive(Debug, Error)]
pub enum SerializationError {
    /// Filesystem operation failed.
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    /// The wrapper accepts only UTF-8 file paths.
    #[error("OpenFHE wrapper requires a UTF-8 path: {0:?}")]
    NonUtf8Path(PathBuf),
    /// Native serializer/deserializer returned false.
    #[error("OpenFHE binary {operation} failed for {path:?}")]
    NativeFailure {
        operation: &'static str,
        path: PathBuf,
    },
    /// A deserialized response produced an unexpected negative slot value.
    #[error("deserialized response slot {position} was negative: {value}")]
    NegativeSlot { position: usize, value: i64 },
}

/// Files written for a complete P0-A client/server serialization measurement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactPaths {
    /// Serialized cryptographic context.
    pub context: PathBuf,
    /// Serialized public encryption key.
    pub public_key: PathBuf,
    /// Serialized client secret key.
    pub private_key: PathBuf,
    /// Serialized encrypted query.
    pub query: PathBuf,
    /// Serialized encrypted response.
    pub response: PathBuf,
}

/// Measured file lengths of `OpenFHE` binary artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactSizes {
    /// Context bytes.
    pub context_bytes: u64,
    /// Public-key bytes.
    pub public_key_bytes: u64,
    /// Secret-key bytes.
    pub private_key_bytes: u64,
    /// Encrypted-query bytes.
    pub query_bytes: u64,
    /// Encrypted-response bytes.
    pub response_bytes: u64,
    /// No evaluation key is generated by this ciphertext--plaintext circuit.
    pub evaluation_key_bytes: Option<u64>,
}

/// Paths and measured sizes from one binary serialization run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerializedBundle {
    /// Artifact paths.
    pub paths: ArtifactPaths,
    /// Artifact file lengths.
    pub sizes: ArtifactSizes,
}

/// Measured setup phases from one context/key construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetupTimings {
    /// Parameter construction, native context generation, and feature enablement.
    pub context_generation_ns: u64,
    /// Public/secret key-pair generation.
    pub key_generation_ns: u64,
}

/// Server-side query-independent packed plaintext database.
pub struct PreparedDatabase {
    plaintext: UniquePtr<ffi::Plaintext>,
}

/// Client-generated encrypted selector. It deliberately carries no clear index.
pub struct EncryptedQuery {
    ciphertext: UniquePtr<ffi::CiphertextDCRTPoly>,
}

/// Server response after the single ciphertext--plaintext multiplication.
pub struct EncryptedResponse {
    ciphertext: UniquePtr<ffi::CiphertextDCRTPoly>,
}

/// Client/server kernel over one `OpenFHE` BGV context and client key pair.
pub struct BgvCpir {
    config: BgvCpirConfig,
    actual_ring_dimension: usize,
    context: UniquePtr<ffi::CryptoContextDCRTPoly>,
    public_key: UniquePtr<ffi::PublicKeyDCRTPoly>,
    private_key: UniquePtr<ffi::PrivateKeyDCRTPoly>,
}

impl BgvCpir {
    /// Returns the version reported by the linked `OpenFHE` core.
    #[must_use]
    pub fn openfhe_version() -> String {
        ffi::GetOpenFHEVersion().to_string_lossy().into_owned()
    }

    /// Constructs a BGV context and client key pair using `OpenFHE`'s standard
    /// 128-bit-classical security setting.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid local packing configuration.
    pub fn new(config: BgvCpirConfig) -> Result<Self, CpirError> {
        Self::new_with_setup_timings(config).map(|(kernel, _)| kernel)
    }

    /// Constructs the kernel and returns separately measured context and key
    /// setup phases.
    ///
    /// # Errors
    ///
    /// Returns the same configuration errors as [`Self::new`].
    pub fn new_with_setup_timings(
        config: BgvCpirConfig,
    ) -> Result<(Self, SetupTimings), CpirError> {
        if config.ring_dimension == 0 {
            return Err(CpirError::InvalidConfiguration(
                "ring dimension must be nonzero",
            ));
        }
        if config.record_slots == 0 {
            return Err(CpirError::InvalidConfiguration(
                "record width must be nonzero",
            ));
        }
        if config.plaintext_modulus < 257 {
            return Err(CpirError::InvalidConfiguration(
                "plaintext modulus must encode every byte",
            ));
        }

        let ring_dimension = u32::try_from(config.ring_dimension)
            .map_err(|_| CpirError::InvalidConfiguration("ring dimension exceeds u32"))?;
        let batch_size = ring_dimension;

        let context_start = Instant::now();
        let mut params = ffi::GenParamsBGVRNS();
        params
            .pin_mut()
            .SetPlaintextModulus(config.plaintext_modulus);
        params.pin_mut().SetRingDim(ring_dimension);
        params.pin_mut().SetBatchSize(batch_size);
        params
            .pin_mut()
            .SetMultiplicativeDepth(config.multiplicative_depth);
        params
            .pin_mut()
            .SetSecurityLevel(ffi::SecurityLevel::HEStd_128_classic);

        let context = ffi::DCRTPolyGenCryptoContextByParamsBGVRNS(&params);
        context.EnableByFeature(ffi::PKESchemeFeature::PKE);
        context.EnableByFeature(ffi::PKESchemeFeature::LEVELEDSHE);

        let actual_ring_dimension = context.GetRingDimension() as usize;
        let context_generation_ns = elapsed_ns(context_start);

        let key_start = Instant::now();
        let key_pair = context.KeyGen();
        let public_key = key_pair.GetPublicKey();
        let private_key = key_pair.GetPrivateKey();
        let key_generation_ns = elapsed_ns(key_start);

        Ok((
            Self {
                config,
                actual_ring_dimension,
                context,
                public_key,
                private_key,
            },
            SetupTimings {
                context_generation_ns,
                key_generation_ns,
            },
        ))
    }

    /// Requested ring dimension.
    #[must_use]
    pub const fn requested_ring_dimension(&self) -> usize {
        self.config.ring_dimension
    }

    /// Actual ring dimension reported by the constructed native context.
    #[must_use]
    pub const fn actual_ring_dimension(&self) -> usize {
        self.actual_ring_dimension
    }

    /// Configured plaintext modulus.
    #[must_use]
    pub const fn plaintext_modulus(&self) -> u64 {
        self.config.plaintext_modulus
    }

    /// Fixed record-window width.
    #[must_use]
    pub const fn record_slots(&self) -> usize {
        self.config.record_slots
    }

    /// Maximum number of physical records in one packed plaintext.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.actual_ring_dimension / self.config.record_slots
    }

    /// Packs one byte per slot into fixed-width record windows and encodes the
    /// query-independent server plaintext.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty/oversized database or record.
    pub fn prepare_database(&self, records: &[Vec<u8>]) -> Result<PreparedDatabase, CpirError> {
        if records.is_empty() {
            return Err(CpirError::EmptyDatabase);
        }
        if records.len() > self.capacity() {
            return Err(CpirError::TooManyRecords {
                actual: records.len(),
                capacity: self.capacity(),
            });
        }

        let mut slots = vec![0_i64; self.actual_ring_dimension];
        for (record_index, record) in records.iter().enumerate() {
            if record.len() > self.config.record_slots {
                return Err(CpirError::RecordTooLong {
                    index: record_index,
                    actual: record.len(),
                    maximum: self.config.record_slots,
                });
            }
            let start = record_index * self.config.record_slots;
            for (offset, &byte) in record.iter().enumerate() {
                slots[start + offset] = i64::from(byte);
            }
        }

        let native_slots = to_cxx_vector(&slots);
        let plaintext = self.context.MakePackedPlaintext(&native_slots, 1, 0);
        Ok(PreparedDatabase { plaintext })
    }

    /// Builds and encrypts one audited record-window selector.
    ///
    /// # Errors
    ///
    /// Returns an error when `index` is outside physical packed capacity.
    pub fn encrypt_query(&self, index: usize) -> Result<EncryptedQuery, CpirError> {
        self.encrypt_window_query(&[index])
    }

    /// Builds and encrypts one binary union of complete record windows.
    ///
    /// This repeats the audited HLF-PIR full-window fill for each distinct
    /// physical index, then packed-encodes and encrypts the full selector once.
    ///
    /// # Errors
    ///
    /// An empty selection is encoded and encrypted as the all-zero selector.
    /// Returns an error for a duplicate index or physical index outside packed
    /// capacity.
    pub fn encrypt_window_query(&self, indices: &[usize]) -> Result<EncryptedQuery, CpirError> {
        let mut selector = vec![0_i64; self.actual_ring_dimension];
        for &index in indices {
            if index >= self.capacity() {
                return Err(CpirError::IndexOutOfRange {
                    index,
                    capacity: self.capacity(),
                });
            }
            let start = index * self.config.record_slots;
            if selector[start] != 0 {
                return Err(CpirError::DuplicateIndex { index });
            }
            selector[start..start + self.config.record_slots].fill(1);
        }
        let native_selector = to_cxx_vector(&selector);
        let plaintext = self.context.MakePackedPlaintext(&native_selector, 1, 0);
        let ciphertext = self
            .context
            .EncryptByPublicKey(&self.public_key, &plaintext);
        Ok(EncryptedQuery { ciphertext })
    }

    /// Performs the one online server ciphertext--plaintext multiplication.
    #[must_use]
    pub fn answer(
        &self,
        database: &mut PreparedDatabase,
        query: &EncryptedQuery,
    ) -> EncryptedResponse {
        let ciphertext = self
            .context
            .EvalMultByCiphertextAndPlaintext(&query.ciphertext, database.plaintext.pin_mut());
        EncryptedResponse { ciphertext }
    }

    /// Decrypts the complete response slot vector.
    ///
    /// # Errors
    ///
    /// Returns an error if `OpenFHE` does not return exactly the configured slot
    /// domain or if a selected value cannot be represented as a nonnegative
    /// integer.
    pub fn decrypt_slots(&self, response: &EncryptedResponse) -> Result<Vec<u64>, CpirError> {
        let mut plaintext = ffi::GenNullPlainText();
        let _decrypt_result = self.context.DecryptByPrivateKeyAndCiphertext(
            &self.private_key,
            &response.ciphertext,
            plaintext.pin_mut(),
        );
        plaintext.SetLength(self.actual_ring_dimension);
        let packed = plaintext.GetPackedValue();
        if packed.len() != self.actual_ring_dimension {
            return Err(CpirError::WrongSlotCount {
                actual: packed.len(),
                expected: self.actual_ring_dimension,
            });
        }

        packed
            .iter()
            .enumerate()
            .map(|(position, &value)| {
                u64::try_from(value).map_err(|_| CpirError::NonByteSlot {
                    position,
                    value: value.unsigned_abs(),
                })
            })
            .collect()
    }

    /// Serializes context, client keys, query, and response using `OpenFHE`'s
    /// binary archive format and measures the resulting file lengths.
    ///
    /// No evaluation key is written because the protocol performs only a
    /// ciphertext--plaintext multiplication.
    ///
    /// # Errors
    ///
    /// Returns an error for filesystem, path-encoding, or native serialization
    /// failure.
    pub fn serialize_binary_bundle(
        &self,
        directory: &Path,
        query: &EncryptedQuery,
        response: &EncryptedResponse,
    ) -> Result<SerializedBundle, SerializationError> {
        fs::create_dir_all(directory)?;
        let paths = ArtifactPaths {
            context: directory.join("context.bin"),
            public_key: directory.join("public_key.bin"),
            private_key: directory.join("private_key.bin"),
            query: directory.join("query.bin"),
            response: directory.join("response.bin"),
        };

        serialize_context(&paths.context, &self.context)?;
        serialize_public_key(&paths.public_key, &self.public_key)?;
        serialize_private_key(&paths.private_key, &self.private_key)?;
        serialize_ciphertext(&paths.query, &query.ciphertext)?;
        serialize_ciphertext(&paths.response, &response.ciphertext)?;

        let sizes = ArtifactSizes {
            context_bytes: fs::metadata(&paths.context)?.len(),
            public_key_bytes: fs::metadata(&paths.public_key)?.len(),
            private_key_bytes: fs::metadata(&paths.private_key)?.len(),
            query_bytes: fs::metadata(&paths.query)?.len(),
            response_bytes: fs::metadata(&paths.response)?.len(),
            evaluation_key_bytes: None,
        };
        Ok(SerializedBundle { paths, sizes })
    }

    /// Serializes generated context as OpenFHE JSON for parameter audit.
    pub fn serialize_context_json(&self, path: &Path) -> Result<(), SerializationError> {
        let native_path_text = utf8_path(path)?;
        openfhe::cxx::let_cxx_string!(native_path = native_path_text);
        native_result(
            ffi::DCRTPolySerializeCryptoContextToFile(
                &native_path,
                &self.context,
                ffi::SerialMode::JSON,
            ),
            "context JSON serialization",
            path,
        )
    }

    /// Deserializes a context, secret key, and response into fresh wrapper
    /// objects and decrypts the full slot vector.
    ///
    /// # Errors
    ///
    /// Returns an error for path/native deserialization failure or a negative
    /// decoded slot.
    pub fn decrypt_serialized_response(
        paths: &ArtifactPaths,
    ) -> Result<Vec<u64>, SerializationError> {
        let mut context = ffi::DCRTPolyGenNullCryptoContext();
        deserialize_context(&paths.context, context.pin_mut())?;
        let mut private_key = ffi::DCRTPolyGenNullPrivateKey();
        deserialize_private_key(&paths.private_key, private_key.pin_mut())?;
        let mut response = ffi::DCRTPolyGenNullCiphertext();
        deserialize_ciphertext(&paths.response, response.pin_mut())?;

        let slot_count = context.GetRingDimension() as usize;
        let mut plaintext = ffi::GenNullPlainText();
        let _decrypt_result =
            context.DecryptByPrivateKeyAndCiphertext(&private_key, &response, plaintext.pin_mut());
        plaintext.SetLength(slot_count);
        plaintext
            .GetPackedValue()
            .iter()
            .enumerate()
            .map(|(position, &value)| {
                u64::try_from(value)
                    .map_err(|_| SerializationError::NegativeSlot { position, value })
            })
            .collect()
    }

    /// Decodes an exact client-known prefix from one selected window, retaining
    /// interior zero bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed slot vector, physical index, length, or
    /// non-byte slot value.
    pub fn decode_fixed_window(
        &self,
        slots: &[u64],
        index: usize,
        length: usize,
    ) -> Result<Vec<u8>, CpirError> {
        self.validate_decode(slots, index, length)?;
        let start = index * self.config.record_slots;
        slots[start..start + length]
            .iter()
            .enumerate()
            .map(|(offset, &value)| {
                u8::try_from(value).map_err(|_| CpirError::NonByteSlot {
                    position: start + offset,
                    value,
                })
            })
            .collect()
    }

    /// Reproduces the prior Go codec by stopping at the first zero slot.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed shape/index or a non-byte slot before the
    /// terminator.
    pub fn decode_legacy_zero_terminated(
        &self,
        slots: &[u64],
        index: usize,
    ) -> Result<Vec<u8>, CpirError> {
        self.validate_decode(slots, index, self.config.record_slots)?;
        let start = index * self.config.record_slots;
        let end = start + self.config.record_slots;
        let mut decoded = Vec::new();
        for (offset, &value) in slots[start..end].iter().enumerate() {
            if value == 0 {
                break;
            }
            decoded.push(u8::try_from(value).map_err(|_| CpirError::NonByteSlot {
                position: start + offset,
                value,
            })?);
        }
        Ok(decoded)
    }

    fn validate_decode(&self, slots: &[u64], index: usize, length: usize) -> Result<(), CpirError> {
        if slots.len() != self.actual_ring_dimension {
            return Err(CpirError::WrongSlotCount {
                actual: slots.len(),
                expected: self.actual_ring_dimension,
            });
        }
        if index >= self.capacity() {
            return Err(CpirError::IndexOutOfRange {
                index,
                capacity: self.capacity(),
            });
        }
        if length > self.config.record_slots {
            return Err(CpirError::DecodeLengthTooLong {
                actual: length,
                maximum: self.config.record_slots,
            });
        }
        Ok(())
    }
}

fn to_cxx_vector(values: &[i64]) -> UniquePtr<CxxVector<i64>> {
    let mut result = CxxVector::<i64>::new();
    for &value in values {
        result.pin_mut().push(value);
    }
    result
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn utf8_path(path: &Path) -> Result<&str, SerializationError> {
    path.to_str()
        .ok_or_else(|| SerializationError::NonUtf8Path(path.to_path_buf()))
}

fn native_result(ok: bool, operation: &'static str, path: &Path) -> Result<(), SerializationError> {
    if ok {
        Ok(())
    } else {
        Err(SerializationError::NativeFailure {
            operation,
            path: path.to_path_buf(),
        })
    }
}

fn serialize_context(
    path: &Path,
    context: &ffi::CryptoContextDCRTPoly,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolySerializeCryptoContextToFile(&native_path, context, ffi::SerialMode::BINARY),
        "context serialization",
        path,
    )
}

fn serialize_public_key(
    path: &Path,
    key: &ffi::PublicKeyDCRTPoly,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolySerializePublicKeyToFile(&native_path, key, ffi::SerialMode::BINARY),
        "public-key serialization",
        path,
    )
}

fn serialize_private_key(
    path: &Path,
    key: &ffi::PrivateKeyDCRTPoly,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolySerializePrivateKeyToFile(&native_path, key, ffi::SerialMode::BINARY),
        "private-key serialization",
        path,
    )
}

fn serialize_ciphertext(
    path: &Path,
    ciphertext: &ffi::CiphertextDCRTPoly,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolySerializeCiphertextToFile(&native_path, ciphertext, ffi::SerialMode::BINARY),
        "ciphertext serialization",
        path,
    )
}

fn deserialize_context(
    path: &Path,
    context: std::pin::Pin<&mut ffi::CryptoContextDCRTPoly>,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolyDeserializeCryptoContextFromFile(
            &native_path,
            context,
            ffi::SerialMode::BINARY,
        ),
        "context deserialization",
        path,
    )
}

fn deserialize_private_key(
    path: &Path,
    key: std::pin::Pin<&mut ffi::PrivateKeyDCRTPoly>,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolyDeserializePrivateKeyFromFile(&native_path, key, ffi::SerialMode::BINARY),
        "private-key deserialization",
        path,
    )
}

fn deserialize_ciphertext(
    path: &Path,
    ciphertext: std::pin::Pin<&mut ffi::CiphertextDCRTPoly>,
) -> Result<(), SerializationError> {
    openfhe::cxx::let_cxx_string!(native_path = utf8_path(path)?);
    native_result(
        ffi::DCRTPolyDeserializeCiphertextFromFile(
            &native_path,
            ciphertext,
            ffi::SerialMode::BINARY,
        ),
        "ciphertext deserialization",
        path,
    )
}
