#![cfg(feature = "openfhe-backend")]

use cbpir_bgv::fixed_window::{mask_slots, multi_window_selector, pack_records, window_selector};
use cbpir_bgv::{BgvCpir, BgvCpirConfig, CpirError};

const RECORD_SLOTS: usize = 128;
const EXPECTED_RING_DIMENSION: usize = 8192;
const CAPACITY: usize = EXPECTED_RING_DIMENSION / RECORD_SLOTS;

fn records() -> Vec<Vec<u8>> {
    (0..CAPACITY)
        .map(|index| {
            let mut record = if index == CAPACITY / 2 {
                b"A\0B".to_vec()
            } else {
                format!("{{\"id\":{index},\"payload\":\"record-{index:02}\"}}").into_bytes()
            };
            record.resize(RECORD_SLOTS, 0);
            record
        })
        .collect()
}

#[test]
fn reports_exact_openfhe_runtime_version() {
    assert_eq!(BgvCpir::openfhe_version(), "1.5.1");
}

#[test]
fn prototype_p0_a_bgv_retrieves_every_window_including_boundaries() {
    let config = BgvCpirConfig::prior_mini();
    let kernel = BgvCpir::new(config).expect("valid audited BGV parameters");

    assert_eq!(kernel.requested_ring_dimension(), EXPECTED_RING_DIMENSION);
    assert_eq!(kernel.actual_ring_dimension(), EXPECTED_RING_DIMENSION);
    assert_eq!(kernel.plaintext_modulus(), 65_537);
    assert_eq!(kernel.record_slots(), RECORD_SLOTS);
    assert_eq!(kernel.capacity(), CAPACITY);

    let records = records();
    let mut prepared = kernel
        .prepare_database(&records)
        .expect("valid full-capacity database");
    let record_refs: Vec<&[u8]> = records.iter().map(Vec::as_slice).collect();
    let packed =
        pack_records(&record_refs, EXPECTED_RING_DIMENSION).expect("oracle packs same database");

    for index in 0..CAPACITY {
        let query = kernel.encrypt_query(index).expect("valid private index");
        let response = kernel.answer(&mut prepared, &query);
        let actual_slots = kernel
            .decrypt_slots(&response)
            .expect("valid decryptable response");

        let selector = window_selector(index, records.len(), RECORD_SLOTS, EXPECTED_RING_DIMENSION)
            .expect("plaintext oracle selector");
        let expected_slots = mask_slots(packed.slots(), &selector)
            .expect("plaintext oracle mask has matching slot count");

        assert_eq!(actual_slots, expected_slots);
        assert_eq!(
            kernel
                .decode_fixed_window(&actual_slots, index, RECORD_SLOTS)
                .expect("known response window"),
            records[index]
        );
    }
}

#[test]
fn prototype_p0_a_rejects_boundary_and_malformed_inputs_before_ffi() {
    let kernel = BgvCpir::new(BgvCpirConfig::prior_mini()).expect("valid parameters");

    assert!(matches!(
        kernel.encrypt_query(CAPACITY),
        Err(CpirError::IndexOutOfRange { .. })
    ));

    let mut too_many = records();
    too_many.push(vec![1]);
    assert!(matches!(
        kernel.prepare_database(&too_many),
        Err(CpirError::TooManyRecords { .. })
    ));

    let mut oversized = records();
    oversized[0].push(1);
    assert!(matches!(
        kernel.prepare_database(&oversized),
        Err(CpirError::RecordTooLong { .. })
    ));

    assert!(matches!(
        kernel.decode_fixed_window(&[0; 4], 0, 1),
        Err(CpirError::WrongSlotCount { .. })
    ));
    assert!(matches!(
        kernel.prepare_database(&[]),
        Err(CpirError::EmptyDatabase)
    ));
    assert!(matches!(
        kernel.decode_fixed_window(&vec![0; EXPECTED_RING_DIMENSION], 0, RECORD_SLOTS + 1),
        Err(CpirError::DecodeLengthTooLong { .. })
    ));

    let mut invalid_config = BgvCpirConfig::prior_mini();
    invalid_config.record_slots = 0;
    assert!(matches!(
        BgvCpir::new(invalid_config),
        Err(CpirError::InvalidConfiguration(_))
    ));
}

#[test]
fn prototype_p0_a_reports_legacy_zero_termination_separately_from_fixed_decode() {
    let kernel = BgvCpir::new(BgvCpirConfig::prior_mini()).expect("valid parameters");
    let mut slots = vec![0_u64; EXPECTED_RING_DIMENSION];
    slots[..4].copy_from_slice(&[65, 0, 66, 0]);

    assert_eq!(
        kernel
            .decode_legacy_zero_terminated(&slots, 0)
            .expect("valid response shape"),
        b"A"
    );
    assert_eq!(
        kernel
            .decode_fixed_window(&slots, 0, 4)
            .expect("valid response shape"),
        b"A\0B\0"
    );
}

#[test]
fn prototype_p0_a_binary_serialization_round_trip_preserves_response() {
    let kernel = BgvCpir::new(BgvCpirConfig::prior_mini()).expect("valid parameters");
    let records = records();
    let mut prepared = kernel.prepare_database(&records).expect("valid database");
    let query = kernel.encrypt_query(CAPACITY - 1).expect("valid index");
    let response = kernel.answer(&mut prepared, &query);
    let directory = tempfile::tempdir().expect("temporary serialization directory");

    let bundle = kernel
        .serialize_binary_bundle(directory.path(), &query, &response)
        .expect("OpenFHE binary serialization succeeds");
    assert!(bundle.sizes.context_bytes > 0);
    assert!(bundle.sizes.public_key_bytes > 0);
    assert!(bundle.sizes.private_key_bytes > 0);
    assert!(bundle.sizes.query_bytes > 0);
    assert!(bundle.sizes.response_bytes > 0);
    assert_eq!(bundle.sizes.evaluation_key_bytes, None);

    let round_trip_slots = BgvCpir::decrypt_serialized_response(&bundle.paths)
        .expect("context, secret key, and response deserialize");
    assert_eq!(
        kernel
            .decode_fixed_window(&round_trip_slots, CAPACITY - 1, RECORD_SLOTS)
            .expect("round-trip response window"),
        records[CAPACITY - 1]
    );
}

#[test]
fn encrypted_multi_window_query_matches_plaintext_union() {
    let kernel = BgvCpir::new(BgvCpirConfig::prior_mini()).expect("valid BGV parameters");
    let records = records();
    let mut database = kernel
        .prepare_database(&records)
        .expect("full-capacity database");
    let indices = [0, 1, CAPACITY - 1];

    let query = kernel
        .encrypt_window_query(&indices)
        .expect("valid distinct windows");
    let response = kernel.answer(&mut database, &query);
    let actual = kernel.decrypt_slots(&response).expect("valid decryption");

    let record_refs = records.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let packed = pack_records(&record_refs, EXPECTED_RING_DIMENSION).expect("oracle packing");
    let selector = multi_window_selector(&indices, CAPACITY, RECORD_SLOTS, EXPECTED_RING_DIMENSION)
        .expect("oracle selector");
    assert_eq!(actual, mask_slots(packed.slots(), &selector).unwrap());
}

#[test]
fn encrypted_empty_window_query_returns_one_all_zero_response() {
    let kernel = BgvCpir::new(BgvCpirConfig::prior_mini()).expect("valid BGV parameters");
    let records = records();
    let mut database = kernel
        .prepare_database(&records)
        .expect("full-capacity database");

    let query = kernel
        .encrypt_window_query(&[])
        .expect("empty set is an encrypted all-zero selector");
    let response = kernel.answer(&mut database, &query);
    let actual = kernel.decrypt_slots(&response).expect("valid decryption");

    assert_eq!(actual, vec![0; EXPECTED_RING_DIMENSION]);
}

#[test]
fn encrypted_multi_window_query_rejects_duplicate_and_out_of_range_ids() {
    let kernel = BgvCpir::new(BgvCpirConfig::prior_mini()).expect("valid BGV parameters");

    assert!(matches!(
        kernel.encrypt_window_query(&[2, 2]),
        Err(CpirError::DuplicateIndex { index: 2 })
    ));
    assert!(matches!(
        kernel.encrypt_window_query(&[CAPACITY]),
        Err(CpirError::IndexOutOfRange { .. })
    ));
}

#[test]
fn fixed_windows_use_floor_capacity_and_leave_nondivisible_tail_zero() {
    let mut config = BgvCpirConfig::prior_mini();
    config.record_slots = 224;
    let kernel = BgvCpir::new(config).expect("224-slot fixed windows need no ring divisibility");
    let capacity = EXPECTED_RING_DIMENSION / 224;
    assert_eq!(kernel.capacity(), capacity);

    let records = (0..capacity)
        .map(|index| format!("fixed-{index}").into_bytes())
        .collect::<Vec<_>>();
    let mut database = kernel.prepare_database(&records).unwrap();
    let query = kernel.encrypt_window_query(&[capacity - 1]).unwrap();
    let response = kernel.answer(&mut database, &query);
    let slots = kernel.decrypt_slots(&response).unwrap();

    assert_eq!(
        kernel
            .decode_fixed_window(&slots, capacity - 1, records[capacity - 1].len())
            .unwrap(),
        records[capacity - 1]
    );
    assert!(slots[capacity * 224..].iter().all(|&value| value == 0));
}
