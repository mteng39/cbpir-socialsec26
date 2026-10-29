use openfhe::cxx::CxxVector;
use openfhe::ffi;

#[test]
fn linked_openfhe_is_1_5_1_and_preserves_complex_slots() {
    assert_eq!(ffi::GetOpenFHEVersion().to_string_lossy(), "1.5.1");

    let mut parameters = ffi::GenParamsCKKSRNS();
    parameters.pin_mut().SetMultiplicativeDepth(1);
    parameters.pin_mut().SetScalingModSize(40);
    parameters.pin_mut().SetFirstModSize(60);
    parameters.pin_mut().SetBatchSize(2);
    parameters
        .pin_mut()
        .SetScalingTechnique(ffi::ScalingTechnique::FIXEDAUTO);
    parameters
        .pin_mut()
        .SetSecurityLevel(ffi::SecurityLevel::HEStd_128_classic);
    parameters
        .pin_mut()
        .SetCKKSDataType(ffi::CKKSDataType::COMPLEX);

    let context = ffi::DCRTPolyGenCryptoContextByParamsCKKSRNS(&parameters);
    context.EnableByFeature(ffi::PKESchemeFeature::PKE);
    let key_pair = context.KeyGen();

    let mut input = CxxVector::<ffi::ComplexPair>::new();
    input.pin_mut().push(ffi::ComplexPair { re: 1.0, im: 2.0 });
    input.pin_mut().push(ffi::ComplexPair { re: 3.0, im: -4.0 });
    let null_parameters = ffi::DCRTPolyGenNullParams();
    let plaintext =
        context.MakeCKKSPackedPlaintextByVectorOfComplex(&input, 1, 0, &null_parameters, 2);
    let ciphertext = context.EncryptByPublicKey(&key_pair.GetPublicKey(), &plaintext);
    let mut decoded = ffi::GenNullPlainText();
    context.DecryptByPrivateKeyAndCiphertext(
        &key_pair.GetPrivateKey(),
        &ciphertext,
        decoded.pin_mut(),
    );
    decoded.SetLength(2);
    let output = decoded
        .GetCopyOfCKKSPackedValue()
        .iter()
        .map(|value| (value.re, value.im))
        .collect::<Vec<_>>();

    assert!((output[0].0 - 1.0).abs() <= 1e-6);
    assert!(
        (output[0].1 - 2.0).abs() <= 1e-6,
        "first decoded slot was ({}, {}), expected (1, 2)",
        output[0].0,
        output[0].1
    );
    assert!((output[1].0 - 3.0).abs() <= 1e-6);
    assert!((output[1].1 + 4.0).abs() <= 1e-6);
}
