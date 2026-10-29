use cbpir_fetch::{
    PHYSICAL_WINDOWS_PER_SHARD, RecordId, SUPPORTED_DATABASE_RECORDS, SUPPORTED_PAYLOAD_BYTES,
    SUPPORTED_PUBLIC_K, ScaledLayout, ScaledP3Error,
};

#[test]
fn all_proposal_payload_database_k_layouts_have_exact_counts() {
    for database_records in SUPPORTED_DATABASE_RECORDS {
        for payload_bytes in SUPPORTED_PAYLOAD_BYTES {
            for public_k in SUPPORTED_PUBLIC_K {
                let layout = ScaledLayout::new(database_records, payload_bytes, public_k).unwrap();
                let real_rows = PHYSICAL_WINDOWS_PER_SHARD - public_k;
                let shards = database_records.div_ceil(real_rows);
                let chunks = payload_bytes / 128;

                assert_eq!(layout.database_records(), database_records);
                assert_eq!(layout.payload_bytes(), payload_bytes);
                assert_eq!(layout.public_k(), public_k);
                assert_eq!(layout.real_rows_per_shard(), real_rows);
                assert_eq!(layout.shard_count(), shards);
                assert_eq!(layout.chunk_count(), chunks);
                assert_eq!(layout.query_ciphertext_count().unwrap(), shards);
                assert_eq!(layout.response_ciphertext_count().unwrap(), shards * chunks);
                assert_eq!(
                    layout.server_multiplication_count().unwrap(),
                    shards * chunks
                );

                let last = layout.map_record(database_records - 1).unwrap();
                assert_eq!(last.shard_index(), shards - 1);
                assert!(last.local_index() < real_rows);
                assert_eq!(
                    layout.shard_real_record_count(shards - 1).unwrap(),
                    database_records - (shards - 1) * real_rows
                );
            }
        }
    }
}

#[test]
fn maximum_database_counts_match_reviewed_derivation() {
    let expected = [
        (1, 261, 2_088),
        (4, 274, 2_192),
        (8, 293, 2_344),
        (16, 342, 2_736),
    ];
    for (public_k, shards, responses_1024) in expected {
        let layout = ScaledLayout::new(1 << 14, 1024, public_k).unwrap();
        assert_eq!(layout.shard_count(), shards);
        assert_eq!(layout.response_ciphertext_count().unwrap(), responses_1024);
    }
}

#[test]
fn unsupported_database_payload_k_and_record_ids_are_rejected() {
    assert!(matches!(
        ScaledLayout::new(512, 256, 4),
        Err(ScaledP3Error::UnsupportedDatabaseRecords { actual: 512 })
    ));
    assert!(matches!(
        ScaledLayout::new(1024, 128, 4),
        Err(ScaledP3Error::UnsupportedPayloadBytes { actual: 128 })
    ));
    assert!(matches!(
        ScaledLayout::new(1024, 256, 2),
        Err(ScaledP3Error::UnsupportedPublicK { actual: 2 })
    ));
    let layout = ScaledLayout::new(1024, 256, 4).unwrap();
    assert!(matches!(
        layout.map_record(1024),
        Err(ScaledP3Error::CandidateOutOfRange { id: 1024, .. })
    ));
    assert!(matches!(
        layout.shard_real_record_count(layout.shard_count()),
        Err(ScaledP3Error::ShardOutOfRange { .. })
    ));
}

#[test]
fn global_plan_preserves_order_and_builds_every_fixed_k_shard() {
    let layout = ScaledLayout::new(1024, 256, 4).unwrap();
    let plan = layout
        .plan(&[
            RecordId(1023),
            RecordId(0),
            RecordId(60),
            RecordId(59),
            RecordId(60),
            RecordId(900),
        ])
        .unwrap();

    assert_eq!(
        plan.selected_ids(),
        &[RecordId(1023), RecordId(0), RecordId(60), RecordId(59)]
    );
    assert_eq!(plan.truncated_unique_count(), 1);
    assert_eq!(plan.shard_plan_count(), layout.shard_count());
    for shard in 0..layout.shard_count() {
        assert_eq!(plan.shard_physical_ids(shard).unwrap().len(), 4);
        let mut unique = plan.shard_physical_ids(shard).unwrap().to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 4);
    }
    assert_eq!(
        plan.shard_selected_global_ids(0).unwrap(),
        &[RecordId(0), RecordId(59)]
    );
    assert_eq!(plan.shard_selected_global_ids(1).unwrap(), &[RecordId(60)]);
    assert_eq!(
        plan.shard_selected_global_ids(17).unwrap(),
        &[RecordId(1023)]
    );
}

#[test]
fn zero_candidates_make_every_shard_all_dummy() {
    let layout = ScaledLayout::new(1024, 1024, 16).unwrap();
    let plan = layout.plan(&[]).unwrap();
    assert!(plan.selected_ids().is_empty());
    for shard in 0..layout.shard_count() {
        assert_eq!(plan.shard_selected_real_count(shard).unwrap(), 0);
        assert_eq!(plan.shard_dummy_count(shard).unwrap(), 16);
    }
}

#[test]
fn invalid_candidate_after_k_prefix_is_rejected_before_truncation() {
    let layout = ScaledLayout::new(1024, 256, 4).unwrap();
    let error = layout
        .plan(&[
            RecordId(0),
            RecordId(1),
            RecordId(2),
            RecordId(3),
            RecordId(1024),
        ])
        .unwrap_err();
    assert!(matches!(
        error,
        ScaledP3Error::CandidateOutOfRange {
            id: 1024,
            database_records: 1024
        }
    ));
}
