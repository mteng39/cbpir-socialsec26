#![cfg(feature = "openfhe-backend")]

use cbpir_bgv::{BgvCpir, BgvProfile};

#[test]
fn named_bgv_profiles_have_expected_capacity() {
    for (profile, expected_capacity) in [
        (BgvProfile::Bgv13, 64),
        (BgvProfile::Bgv14, 128),
        (BgvProfile::Bgv15, 256),
    ] {
        let config = profile.config();
        assert_eq!(config.ring_dimension, profile.ring_dimension());
        assert_eq!(config.plaintext_modulus, 65_537);
        assert_eq!(config.record_slots, 128);
        assert_eq!(config.multiplicative_depth, 1);

        let kernel = BgvCpir::new(config).unwrap();
        assert_eq!(kernel.actual_ring_dimension(), profile.ring_dimension());
        assert_eq!(kernel.capacity(), expected_capacity);
    }
}

#[test]
fn every_bgv_profile_runs_full_multi_window_fetch() {
    for profile in [BgvProfile::Bgv13, BgvProfile::Bgv14, BgvProfile::Bgv15] {
        let kernel = BgvCpir::new(profile.config()).unwrap();
        let mut records = vec![vec![0_u8; 128]; kernel.capacity()];
        records[0][..3].copy_from_slice(b"one");
        records[kernel.capacity() / 2][..3].copy_from_slice(b"mid");
        records[kernel.capacity() - 1][..3].copy_from_slice(b"end");
        let mut database = kernel.prepare_database(&records).unwrap();
        let selected = [0, kernel.capacity() / 2, kernel.capacity() - 1];
        let query = kernel.encrypt_window_query(&selected).unwrap();
        let response = kernel.answer(&mut database, &query);
        let slots = kernel.decrypt_slots(&response).unwrap();

        for index in 0..kernel.capacity() {
            let window = &slots[index * 128..(index + 1) * 128];
            if selected.contains(&index) {
                let expected = records[index]
                    .iter()
                    .copied()
                    .map(u64::from)
                    .collect::<Vec<_>>();
                assert_eq!(window, expected);
            } else {
                assert!(window.iter().all(|&value| value == 0));
            }
        }
    }
}
