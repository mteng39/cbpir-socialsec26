use cbpir_bgv::BgvCpirConfig;
use cbpir_fetch::{RecordId, ScaledLayout, ScaledP3Error, ShardedP3BgvBackend};

fn record(id: usize, payload_bytes: usize) -> Vec<u8> {
    if id == 0 {
        return vec![0; payload_bytes];
    }
    let id_u64 = u64::try_from(id).expect("fixture ID fits u64");
    let mut bytes = (0..payload_bytes)
        .map(|offset| {
            let offset_u64 = u64::try_from(offset).expect("fixture offset fits u64");
            let quotient_u64 = u64::try_from(offset / 7).expect("fixture quotient fits u64");
            u8::try_from((id_u64 * 131 + offset_u64 * 17 + quotient_u64) % 251)
                .expect("fixture byte is below 251")
        })
        .collect::<Vec<_>>();
    bytes[..8].copy_from_slice(&id_u64.to_le_bytes());
    bytes[8] = 0;
    bytes[payload_bytes / 2] = 0;
    bytes[payload_bytes - 1] = 0;
    bytes
}

fn records(count: usize, payload_bytes: usize) -> Vec<Vec<u8>> {
    (0..count).map(|id| record(id, payload_bytes)).collect()
}

fn assert_fetched(
    result: &cbpir_fetch::ScaledFetchResult,
    expected_ids: &[RecordId],
    source: &[Vec<u8>],
) {
    assert_eq!(result.records.len(), expected_ids.len());
    for (actual, &id) in result.records.iter().zip(expected_ids) {
        assert_eq!(actual.id(), id);
        assert_eq!(
            actual.bytes(),
            source[usize::try_from(id.0).expect("fixture ID fits usize")]
        );
    }
}

#[test]
fn encrypted_256_byte_fetch_restores_same_and_cross_shard_order() {
    let source = records(1024, 256);
    let (mut backend, setup) =
        ShardedP3BgvBackend::new(source.clone(), 4, BgvCpirConfig::prior_mini()).unwrap();
    let ids = [RecordId(1023), RecordId(0), RecordId(60), RecordId(59)];
    let plan = backend.plan(&ids).unwrap();
    let result = backend.fetch(&plan).unwrap();

    assert_eq!(result.records.len(), ids.len());
    for (actual, id) in result.records.iter().zip(ids) {
        assert_eq!(actual.id(), id);
        assert_eq!(
            actual.bytes(),
            source[usize::try_from(id.0).expect("fixture ID fits usize")]
        );
    }
    assert!(setup.context_generation_ns > 0);
    assert!(setup.key_generation_ns > 0);
    assert_eq!(result.counts.query_ciphertexts, 18);
    assert_eq!(result.counts.response_ciphertexts, 36);
    assert_eq!(result.counts.server_multiplications, 36);
    assert_eq!(result.counts.evaluated_chunk_planes, 36);
}

#[test]
fn encrypted_1024_byte_fetch_reconstructs_first_and_last_payloads() {
    let source = records(1024, 1024);
    let (mut backend, _) =
        ShardedP3BgvBackend::new(source.clone(), 4, BgvCpirConfig::prior_mini()).unwrap();
    let ids = [RecordId(0), RecordId(1023)];
    let plan = backend.plan(&ids).unwrap();
    let result = backend.fetch(&plan).unwrap();
    assert_eq!(result.records[0].bytes(), source[0]);
    assert_eq!(result.records[1].bytes(), source[1023]);
    assert_eq!(result.counts.query_ciphertexts, 18);
    assert_eq!(result.counts.response_ciphertexts, 144);
}

#[test]
fn zero_fewer_exact_and_over_k_candidates_preserve_declared_semantics() {
    let source = records(1024, 256);
    let (mut backend, _) =
        ShardedP3BgvBackend::new(source.clone(), 4, BgvCpirConfig::prior_mini()).unwrap();

    let empty_plan = backend.plan(&[]).unwrap();
    let empty = backend.fetch(&empty_plan).unwrap();
    assert!(empty.records.is_empty());
    assert_eq!(empty.counts.query_ciphertexts, 18);
    assert_eq!(empty.counts.response_ciphertexts, 36);

    let fewer_ids = [RecordId(0), RecordId(1023)];
    let fewer_plan = backend.plan(&fewer_ids).unwrap();
    let fewer = backend.fetch(&fewer_plan).unwrap();
    assert_fetched(&fewer, &fewer_ids, &source);

    let exact_ids = [RecordId(0), RecordId(59), RecordId(60), RecordId(1023)];
    let exact_plan = backend.plan(&exact_ids).unwrap();
    let exact = backend.fetch(&exact_plan).unwrap();
    assert_fetched(&exact, &exact_ids, &source);

    let over_candidates = [
        RecordId(1023),
        RecordId(0),
        RecordId(1023),
        RecordId(60),
        RecordId(59),
        RecordId(900),
    ];
    let over_plan = backend.plan(&over_candidates).unwrap();
    assert_eq!(over_plan.truncated_unique_count(), 1);
    let over = backend.fetch(&over_plan).unwrap();
    assert_fetched(
        &over,
        &[RecordId(1023), RecordId(0), RecordId(60), RecordId(59)],
        &source,
    );
}

#[test]
fn malformed_ids_width_config_and_plan_layout_are_rejected() {
    let layout = ScaledLayout::new(1024, 256, 4).unwrap();
    let invalid = layout
        .plan(&[
            RecordId(0),
            RecordId(1),
            RecordId(2),
            RecordId(3),
            RecordId(1024),
        ])
        .unwrap_err();
    assert!(matches!(
        invalid,
        ScaledP3Error::CandidateOutOfRange {
            id: 1024,
            database_records: 1024
        }
    ));

    let mut wrong_width = records(1024, 256);
    wrong_width[1].pop();
    let wrong_width_result = ShardedP3BgvBackend::new(wrong_width, 4, BgvCpirConfig::prior_mini());
    assert!(matches!(
        wrong_width_result,
        Err(ScaledP3Error::RecordWidth {
            index: 1,
            actual: 255,
            expected: 256
        })
    ));

    let mut wrong_config = BgvCpirConfig::prior_mini();
    wrong_config.record_slots = 256;
    let wrong_config_result = ShardedP3BgvBackend::new(records(1024, 256), 4, wrong_config);
    assert!(matches!(
        wrong_config_result,
        Err(ScaledP3Error::UnsupportedKernelConfig)
    ));

    let (mut backend, _) =
        ShardedP3BgvBackend::new(records(1024, 1024), 4, BgvCpirConfig::prior_mini()).unwrap();
    let foreign_plan = layout.plan(&[RecordId(0)]).unwrap();
    assert!(matches!(
        backend.fetch(&foreign_plan),
        Err(ScaledP3Error::PlanLayoutMismatch { .. })
    ));
}

#[test]
fn representative_scaled_exchange_round_trips_and_removes_artifacts() {
    let source = records(1024, 256);
    let (backend, _) = ShardedP3BgvBackend::new(source, 4, BgvCpirConfig::prior_mini()).unwrap();
    let plan = backend.plan(&[RecordId(0), RecordId(1023)]).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let sample = backend
        .serialize_representative(&plan, directory.path())
        .unwrap();

    assert!(sample.context_bytes > 0);
    assert!(sample.public_key_bytes > 0);
    assert!(sample.private_key_bytes > 0);
    assert!(sample.query_bytes > 0);
    assert!(sample.response_bytes > 0);
    assert_eq!(sample.evaluation_key_bytes, None);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
