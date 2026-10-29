use cbpir_bgv::fixed_window::{
    FixedWindowError, decode_window, mask_slots, multi_window_selector, pack_records,
    window_selector,
};

#[test]
fn fixed_window_oracle_retrieves_a_known_middle_record() {
    let records: [&[u8]; 3] = [br#"{"id":0}"#, br#"{"id":1}"#, br#"{"id":2}"#];
    let packed = pack_records(&records, 32).expect("three eight-byte records fit");
    let selector = window_selector(
        1,
        packed.record_count(),
        packed.slots_per_record(),
        packed.slots().len(),
    )
    .expect("middle index is valid");
    let response = mask_slots(packed.slots(), &selector).expect("slot domains match");

    assert_eq!(packed.slots_per_record(), 8);
    assert_eq!(selector.iter().copied().sum::<u64>(), 8);
    assert_eq!(
        decode_window(&response, 1, packed.slots_per_record()),
        Ok(br#"{"id":1}"#.to_vec())
    );
}

#[test]
fn fixed_window_oracle_retrieves_every_representative_index() {
    let records: [&[u8]; 3] = [b"first", b"middle", b"last"];
    let packed = pack_records(&records, 32).expect("three rounded windows fit");

    for (index, expected) in records.into_iter().enumerate() {
        let selector = window_selector(
            index,
            packed.record_count(),
            packed.slots_per_record(),
            packed.slots().len(),
        )
        .expect("index is valid");
        let response = mask_slots(packed.slots(), &selector).expect("slot domains match");

        assert_eq!(
            decode_window(&response, index, packed.slots_per_record()),
            Ok(expected.to_vec())
        );
    }
}

#[test]
fn pack_records_rejects_capacity_overflow() {
    let records: [&[u8]; 2] = [b"12345678", b"123456789"];

    assert_eq!(
        pack_records(&records, 16),
        Err(FixedWindowError::CapacityExceeded {
            required: 32,
            available: 16,
        })
    );
}

#[test]
fn window_selector_rejects_index_equal_to_record_count() {
    assert_eq!(
        window_selector(3, 3, 8, 24),
        Err(FixedWindowError::IndexOutOfBounds {
            index: 3,
            record_count: 3,
        })
    );
}

#[test]
fn mask_slots_rejects_mismatched_slot_domains() {
    assert_eq!(
        mask_slots(&[65, 66], &[1]),
        Err(FixedWindowError::LengthMismatch {
            database_slots: 2,
            selector_slots: 1,
        })
    );
}

#[test]
fn zero_terminated_codec_truncates_an_interior_zero_byte() {
    let records: [&[u8]; 1] = [b"A\0B"];
    let packed = pack_records(&records, 8).expect("one rounded window fits");
    let selector = window_selector(0, 1, 8, 8).expect("index is valid");
    let response = mask_slots(packed.slots(), &selector).expect("slot domains match");

    assert_eq!(decode_window(&response, 0, 8), Ok(b"A".to_vec()));
}

#[test]
fn zero_width_layout_is_rejected_at_every_public_layout_boundary() {
    let empty_record: [&[u8]; 1] = [b""];

    assert_eq!(
        pack_records(&empty_record, 8),
        Err(FixedWindowError::ZeroWindowWidth)
    );
    assert_eq!(
        window_selector(0, 1, 0, 8),
        Err(FixedWindowError::ZeroWindowWidth)
    );
    assert_eq!(
        decode_window(&[0; 8], 0, 0),
        Err(FixedWindowError::ZeroWindowWidth)
    );
}

#[test]
fn go_derived_rounding_fixtures_cover_lengths_eight_nine_and_sixteen() {
    let length_eight: [&[u8]; 1] = [b"12345678"];
    let packed = pack_records(&length_eight, 16).expect("eight bytes use one window");
    assert_eq!(packed.slots_per_record(), 8);
    assert_eq!(
        packed.slots(),
        &[49, 50, 51, 52, 53, 54, 55, 56, 0, 0, 0, 0, 0, 0, 0, 0]
    );

    let length_nine: [&[u8]; 1] = [b"123456789"];
    let packed = pack_records(&length_nine, 16).expect("nine bytes round to sixteen slots");
    assert_eq!(packed.slots_per_record(), 16);
    assert_eq!(
        packed.slots(),
        &[49, 50, 51, 52, 53, 54, 55, 56, 57, 0, 0, 0, 0, 0, 0, 0]
    );

    let length_sixteen: [&[u8]; 1] = [b"abcdefghijklmnop"];
    let packed = pack_records(&length_sixteen, 16).expect("sixteen bytes fill one window");
    assert_eq!(packed.slots_per_record(), 16);
    assert_eq!(
        packed.slots(),
        &[
            97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112
        ]
    );
}

#[test]
fn go_derived_heterogeneous_fixture_has_exact_windows_and_tail_padding() {
    let records: [&[u8]; 3] = [b"", b"AB", b"123456789"];
    let packed = pack_records(&records, 64).expect("three sixteen-slot windows fit");

    assert_eq!(packed.record_count(), 3);
    assert_eq!(packed.slots_per_record(), 16);
    assert_eq!(
        packed.slots(),
        &[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 49, 50, 51, 52, 53, 54, 55, 56, 57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]
    );
}

#[test]
fn go_derived_selector_fixture_is_exact_and_independent_of_packing() {
    let selector = window_selector(1, 3, 16, 64).expect("middle window is valid");
    let expected = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0,
    ];

    assert_eq!(selector, expected);
}

#[test]
fn multi_window_selector_matches_repeated_hlf_window_fill() {
    let selector = multi_window_selector(&[0, 2, 3], 4, 4, 16).expect("valid windows");

    assert_eq!(
        selector,
        vec![1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1]
    );
    assert_eq!(selector.iter().sum::<u64>(), 3 * 4);
}

#[test]
fn multi_window_selector_encodes_empty_as_all_zero_and_rejects_duplicates() {
    assert_eq!(multi_window_selector(&[], 4, 4, 16).unwrap(), vec![0; 16]);
    assert_eq!(
        multi_window_selector(&[1, 1], 4, 4, 16),
        Err(FixedWindowError::DuplicateIndex { index: 1 })
    );
}

#[test]
fn multi_window_selector_covers_first_and_last_without_wraparound() {
    let selector = multi_window_selector(&[0, 3], 4, 4, 16).expect("valid boundaries");

    assert_eq!(&selector[..4], &[1; 4]);
    assert_eq!(&selector[12..], &[1; 4]);
    assert!(selector[4..12].iter().all(|&slot| slot == 0));
}

#[test]
fn go_derived_mask_fixture_is_exact_and_independent_of_other_oracles() {
    let database = [10, 20, 30, 40, 50, 60, 70, 80];
    let selector = [0, 0, 1, 1, 0, 0, 0, 0];

    assert_eq!(
        mask_slots(&database, &selector),
        Ok(vec![0, 0, 30, 40, 0, 0, 0, 0])
    );
}

#[test]
fn go_derived_decode_fixture_is_exact_and_rejects_non_byte_values() {
    let response = [
        0, 0, 0, 0, 0, 0, 0, 0, 65, 66, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(decode_window(&response, 1, 8), Ok(b"AB".to_vec()));

    assert_eq!(
        decode_window(&[65, 256, 0, 0, 0, 0, 0, 0], 0, 8),
        Err(FixedWindowError::NonByteSlot {
            position: 1,
            value: 256,
        })
    );
}
