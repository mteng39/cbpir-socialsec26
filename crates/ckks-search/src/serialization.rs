use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use num_complex::Complex64;
use openfhe::cxx::{UniquePtr, let_cxx_string};
use openfhe::ffi;

use crate::ckks::{CkksError, CkksSession, as_cxx_complex, exact_fps_scores, exact_rfps_scores};
use crate::layout::complex_dft;
use crate::{CkksConfig, fps_time_layout, idft, rfps_time_layout, right_rotation_factors};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedArtifactSizes {
    pub context_bytes: u64,
    pub public_key_bytes: u64,
    pub private_key_bytes: u64,
    pub eval_mult_key_bytes: u64,
    pub setup_ciphertext_bytes: Vec<u64>,
    pub query_ciphertext_bytes: Vec<u64>,
    pub response_ciphertext_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CkksSerializationResult {
    pub scores: Vec<f64>,
    pub matching_positions: Vec<usize>,
    pub max_abs_error: f64,
    pub sizes: SerializedArtifactSizes,
}

struct ArtifactDirectory {
    path: PathBuf,
}

struct RestoredCommonArtifacts {
    context_bytes: u64,
    public_key_bytes: u64,
    private_key_bytes: u64,
    eval_mult_key_bytes: u64,
    context: UniquePtr<ffi::CryptoContextDCRTPoly>,
    private_key: UniquePtr<ffi::PrivateKeyDCRTPoly>,
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
                "p0c-{protocol}-serialization-{}-{nonce}",
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

fn validate_threshold(threshold: f64) -> Result<(), CkksError> {
    if threshold.is_finite() && threshold >= 0.0 {
        Ok(())
    } else {
        Err(CkksError::InvalidDecisionThreshold)
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
            artifact: "crypto context",
        });
    }
    file_size(Path::new(path.as_ref()))
}

fn serialize_public_key(path: &Path, key: &ffi::PublicKeyDCRTPoly) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializePublicKeyToFile(&path_cxx, key, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure {
            artifact: "public key",
        });
    }
    file_size(Path::new(path.as_ref()))
}

fn serialize_private_key(path: &Path, key: &ffi::PrivateKeyDCRTPoly) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializePrivateKeyToFile(&path_cxx, key, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure {
            artifact: "private key",
        });
    }
    file_size(Path::new(path.as_ref()))
}

fn serialize_eval_mult_key(
    path: &Path,
    context: &ffi::CryptoContextDCRTPoly,
) -> Result<u64, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolySerializeEvalMultKeyToFile(&path_cxx, context, ffi::SerialMode::BINARY) {
        return Err(CkksError::SerializationFailure {
            artifact: "evaluation multiplication key",
        });
    }
    file_size(Path::new(path.as_ref()))
}

fn serialize_ciphertext(
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

fn deserialize_context(path: &Path) -> Result<UniquePtr<ffi::CryptoContextDCRTPoly>, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    let mut context = ffi::DCRTPolyGenNullCryptoContext();
    if !ffi::DCRTPolyDeserializeCryptoContextFromFile(
        &path_cxx,
        context.pin_mut(),
        ffi::SerialMode::BINARY,
    ) {
        return Err(CkksError::DeserializationFailure {
            artifact: "crypto context",
        });
    }
    Ok(context)
}

fn deserialize_public_key(path: &Path) -> Result<UniquePtr<ffi::PublicKeyDCRTPoly>, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    let mut key = ffi::DCRTPolyGenNullPublicKey();
    if !ffi::DCRTPolyDeserializePublicKeyFromFile(&path_cxx, key.pin_mut(), ffi::SerialMode::BINARY)
    {
        return Err(CkksError::DeserializationFailure {
            artifact: "public key",
        });
    }
    Ok(key)
}

fn deserialize_private_key(path: &Path) -> Result<UniquePtr<ffi::PrivateKeyDCRTPoly>, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    let mut key = ffi::DCRTPolyGenNullPrivateKey();
    if !ffi::DCRTPolyDeserializePrivateKeyFromFile(
        &path_cxx,
        key.pin_mut(),
        ffi::SerialMode::BINARY,
    ) {
        return Err(CkksError::DeserializationFailure {
            artifact: "private key",
        });
    }
    Ok(key)
}

fn deserialize_eval_mult_key(path: &Path) -> Result<(), CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    if !ffi::DCRTPolyDeserializeEvalMultKeyFromFile(&path_cxx, ffi::SerialMode::BINARY) {
        return Err(CkksError::DeserializationFailure {
            artifact: "evaluation multiplication key",
        });
    }
    Ok(())
}

fn deserialize_ciphertext(
    path: &Path,
    artifact: &'static str,
) -> Result<UniquePtr<ffi::CiphertextDCRTPoly>, CkksError> {
    let path = path.to_string_lossy();
    let_cxx_string!(path_cxx = path.as_ref());
    let mut ciphertext = ffi::DCRTPolyGenNullCiphertext();
    if !ffi::DCRTPolyDeserializeCiphertextFromFile(
        &path_cxx,
        ciphertext.pin_mut(),
        ffi::SerialMode::BINARY,
    ) {
        return Err(CkksError::DeserializationFailure { artifact });
    }
    Ok(ciphertext)
}

fn plaintext(
    context: &ffi::CryptoContextDCRTPoly,
    config: CkksConfig,
    values: &[Complex64],
) -> UniquePtr<ffi::Plaintext> {
    let values = as_cxx_complex(values);
    context.MakeCKKSPackedPlaintextByVectorOfComplex(
        &values,
        1,
        0,
        &ffi::DCRTPolyGenNullParams(),
        config.batch_size,
    )
}

fn decrypt(
    context: &ffi::CryptoContextDCRTPoly,
    private_key: &ffi::PrivateKeyDCRTPoly,
    ciphertext: &ffi::CiphertextDCRTPoly,
    length: usize,
) -> Vec<Complex64> {
    let mut decoded = ffi::GenNullPlainText();
    context.DecryptByPrivateKeyAndCiphertext(private_key, ciphertext, decoded.pin_mut());
    decoded.SetLength(length);
    decoded
        .GetCopyOfCKKSPackedValue()
        .iter()
        .map(|value| Complex64::new(value.re, value.im))
        .collect()
}

fn common_sizes_and_restore(
    directory: &ArtifactDirectory,
    session: &CkksSession,
) -> Result<RestoredCommonArtifacts, CkksError> {
    let context_path = directory.file("context.bin");
    let public_key_path = directory.file("public-key.bin");
    let private_key_path = directory.file("private-key.bin");
    let eval_mult_key_path = directory.file("eval-mult-key.bin");
    let context_bytes = serialize_context(&context_path, &session.context)?;
    let public_key_bytes =
        serialize_public_key(&public_key_path, &session.key_pair.GetPublicKey())?;
    let private_key_bytes =
        serialize_private_key(&private_key_path, &session.key_pair.GetPrivateKey())?;
    let eval_mult_key_bytes = serialize_eval_mult_key(&eval_mult_key_path, &session.context)?;

    ffi::DCRTPolyClearEvalMultKeys();
    let restored_context = deserialize_context(&context_path)?;
    let _restored_public_key = deserialize_public_key(&public_key_path)?;
    let restored_private_key = deserialize_private_key(&private_key_path)?;
    deserialize_eval_mult_key(&eval_mult_key_path)?;

    Ok(RestoredCommonArtifacts {
        context_bytes,
        public_key_bytes,
        private_key_bytes,
        eval_mult_key_bytes,
        context: restored_context,
        private_key: restored_private_key,
    })
}

fn artifact_sizes(
    restored: &RestoredCommonArtifacts,
    setup_ciphertext_bytes: Vec<u64>,
    query_ciphertext_bytes: Vec<u64>,
    response_ciphertext_bytes: u64,
) -> SerializedArtifactSizes {
    SerializedArtifactSizes {
        context_bytes: restored.context_bytes,
        public_key_bytes: restored.public_key_bytes,
        private_key_bytes: restored.private_key_bytes,
        eval_mult_key_bytes: restored.eval_mult_key_bytes,
        setup_ciphertext_bytes,
        query_ciphertext_bytes,
        response_ciphertext_bytes,
    }
}

/// Serializes and reloads every FPS protocol artifact, then reruns the search.
///
/// # Errors
///
/// Returns [`CkksError`] for an invalid search/threshold, failed file operation,
/// or failed `OpenFHE` serialization operation.
pub fn ckks_fps_serialization_roundtrip(
    text: &[u8],
    pattern: &[u8],
    decision_threshold: f64,
) -> Result<CkksSerializationResult, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = fps_time_layout(text, pattern)?;
    let config = CkksConfig::for_fps_transform_len(layout.transform_len)?;
    let session = CkksSession::new(config);
    let directory = ArtifactDirectory::new("fps")?;

    let encrypted_text = session.encrypt(&complex_dft(&layout.text));
    let encrypted_prefix = session.encrypt(&complex_dft(&layout.prefix_squares));
    let encrypted_pattern = session.encrypt(&complex_dft(&layout.reversed_pattern));
    let encrypted_pattern_square = session.encrypt(&complex_dft(&vec![
        layout.pattern_square_sum;
        layout.transform_len
    ]));

    let setup_paths = [
        directory.file("setup-text.bin"),
        directory.file("setup-prefix.bin"),
    ];
    let query_paths = [
        directory.file("query-pattern.bin"),
        directory.file("query-pattern-square.bin"),
    ];
    let setup_ciphertext_bytes = vec![
        serialize_ciphertext(&setup_paths[0], &encrypted_text, "FPS setup text")?,
        serialize_ciphertext(&setup_paths[1], &encrypted_prefix, "FPS setup prefix")?,
    ];
    let query_ciphertext_bytes = vec![
        serialize_ciphertext(&query_paths[0], &encrypted_pattern, "FPS query pattern")?,
        serialize_ciphertext(
            &query_paths[1],
            &encrypted_pattern_square,
            "FPS query pattern square",
        )?,
    ];
    let restored = common_sizes_and_restore(&directory, &session)?;

    let restored_text = deserialize_ciphertext(&setup_paths[0], "FPS setup text")?;
    let restored_prefix = deserialize_ciphertext(&setup_paths[1], "FPS setup prefix")?;
    let restored_pattern = deserialize_ciphertext(&query_paths[0], "FPS query pattern")?;
    let restored_pattern_square =
        deserialize_ciphertext(&query_paths[1], "FPS query pattern square")?;
    let mut rotation_plaintext = plaintext(
        &restored.context,
        config,
        &right_rotation_factors(layout.transform_len, pattern.len()),
    );
    let rotated_prefix = restored
        .context
        .EvalMultByCiphertextAndPlaintext(&restored_prefix, rotation_plaintext.pin_mut());
    let cross = restored
        .context
        .EvalMultAndRelinearize(&restored_text, &restored_pattern);
    let doubled_cross = restored.context.EvalAddByCiphertexts(&cross, &cross);
    let unscaled_additive_terms = restored
        .context
        .EvalAddByCiphertexts(&restored_prefix, &restored_pattern_square);
    let additive_terms = restored
        .context
        .EvalMultByCiphertextAndConst(&unscaled_additive_terms, 1.0);
    let without_rotated_prefix = restored
        .context
        .EvalSubByCiphertexts(&additive_terms, &rotated_prefix);
    let unscaled_response = restored
        .context
        .EvalSubByCiphertexts(&without_rotated_prefix, &doubled_cross);
    let response = restored.context.Rescale(&unscaled_response);
    let response_path = directory.file("response.bin");
    let response_ciphertext_bytes =
        serialize_ciphertext(&response_path, &response, "FPS response")?;
    let restored_response = deserialize_ciphertext(&response_path, "FPS response")?;

    let decoded = decrypt(
        &restored.context,
        &restored.private_key,
        &restored_response,
        layout.transform_len,
    );
    let time_scores = idft(&decoded);
    let scores = layout
        .valid_coefficients
        .map(|index| time_scores[index].re)
        .collect::<Vec<_>>();
    let exact_scores = exact_fps_scores(text, pattern);
    let max_abs_error = scores
        .iter()
        .zip(exact_scores)
        .map(|(approximate, exact)| (approximate - exact).abs())
        .fold(0.0_f64, f64::max);
    let matching_positions = scores
        .iter()
        .enumerate()
        .filter_map(|(index, score)| (score.abs() <= decision_threshold).then_some(index))
        .collect();

    Ok(CkksSerializationResult {
        scores,
        matching_positions,
        max_abs_error,
        sizes: artifact_sizes(
            &restored,
            setup_ciphertext_bytes,
            query_ciphertext_bytes,
            response_ciphertext_bytes,
        ),
    })
}

/// Serializes and reloads every RFPS protocol artifact, then reruns the search.
///
/// # Errors
///
/// Returns [`CkksError`] for an invalid search/threshold, failed file operation,
/// or failed `OpenFHE` serialization operation.
#[allow(clippy::cast_precision_loss)] // Declared RFPS score bound is below 2^53.
pub fn ckks_rfps_serialization_roundtrip(
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
    decision_threshold: f64,
) -> Result<CkksSerializationResult, CkksError> {
    validate_threshold(decision_threshold)?;
    let layout = rfps_time_layout(text, pattern, randomizer)?;
    let config = CkksConfig::for_rfps_transform_len(layout.transform_len)?;
    let session = CkksSession::new(config);
    let directory = ArtifactDirectory::new("rfps")?;

    let encrypted_text = session.encrypt(&complex_dft(&layout.text));
    let encrypted_randomizer = session.encrypt(&complex_dft(&layout.reversed_randomizer));
    let encrypted_pattern_randomizer = session.encrypt(&complex_dft(&vec![
        layout.pattern_randomizer_dot_product;
        layout.transform_len
    ]));
    let setup_paths = [directory.file("setup-text.bin")];
    let query_paths = [
        directory.file("query-randomizer.bin"),
        directory.file("query-pattern-randomizer.bin"),
    ];
    let setup_ciphertext_bytes = vec![serialize_ciphertext(
        &setup_paths[0],
        &encrypted_text,
        "RFPS setup text",
    )?];
    let query_ciphertext_bytes = vec![
        serialize_ciphertext(
            &query_paths[0],
            &encrypted_randomizer,
            "RFPS query randomizer",
        )?,
        serialize_ciphertext(
            &query_paths[1],
            &encrypted_pattern_randomizer,
            "RFPS query pattern-randomizer dot product",
        )?,
    ];
    let restored = common_sizes_and_restore(&directory, &session)?;

    let restored_text = deserialize_ciphertext(&setup_paths[0], "RFPS setup text")?;
    let restored_randomizer = deserialize_ciphertext(&query_paths[0], "RFPS query randomizer")?;
    let restored_pattern_randomizer =
        deserialize_ciphertext(&query_paths[1], "RFPS query pattern-randomizer dot product")?;
    let cross = restored
        .context
        .EvalMultAndRelinearize(&restored_text, &restored_randomizer);
    let scaled_pattern_randomizer = restored
        .context
        .EvalMultByCiphertextAndConst(&restored_pattern_randomizer, 1.0);
    let response = restored
        .context
        .EvalSubByCiphertexts(&cross, &scaled_pattern_randomizer);
    let response_path = directory.file("response.bin");
    let response_ciphertext_bytes =
        serialize_ciphertext(&response_path, &response, "RFPS response")?;
    let restored_response = deserialize_ciphertext(&response_path, "RFPS response")?;

    let decoded = decrypt(
        &restored.context,
        &restored.private_key,
        &restored_response,
        layout.transform_len,
    );
    let time_scores = idft(&decoded);
    let scores = layout
        .valid_coefficients
        .map(|index| time_scores[index].re)
        .collect::<Vec<_>>();
    let exact_scores = exact_rfps_scores(text, pattern, randomizer);
    let max_abs_error = scores
        .iter()
        .zip(exact_scores)
        .map(|(approximate, exact)| (approximate - exact as f64).abs())
        .fold(0.0_f64, f64::max);
    let matching_positions = scores
        .iter()
        .enumerate()
        .filter_map(|(index, score)| (score.abs() <= decision_threshold).then_some(index))
        .collect();

    Ok(CkksSerializationResult {
        scores,
        matching_positions,
        max_abs_error,
        sizes: artifact_sizes(
            &restored,
            setup_ciphertext_bytes,
            query_ciphertext_bytes,
            response_ciphertext_bytes,
        ),
    })
}
