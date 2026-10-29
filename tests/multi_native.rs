#![cfg(feature = "native")]

use cbpir::{LogicalRecord, MatchMode, MatchRequest, MultiPipeline, RecordId};

fn record(id: u64, searchable: &[u8]) -> LogicalRecord {
    LogicalRecord {
        id: RecordId::new(id).unwrap(),
        searchable: searchable.to_vec(),
        payload: format!("payload-for-stable-id-{id}").into_bytes(),
    }
}

fn fps(pattern: &[u8]) -> MatchRequest {
    MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Fps,
    }
}

#[test]
fn k4_pipeline_maps_arbitrary_stable_ids_to_physical_rows_and_back() {
    let records = vec![
        record(9_001, b"solo common"),
        record(101, b"common"),
        record(7_007, b"common"),
        record(3_003, b"common"),
        record(5_005, b"other"),
    ];
    let expected_payload = |id| {
        records
            .iter()
            .find(|record| record.id.get() == id)
            .unwrap()
            .payload
            .clone()
    };
    let mut pipeline = MultiPipeline::prepare(records.clone(), 4).unwrap();

    let exact_k = pipeline.run(&fps(b"common")).unwrap();
    assert_eq!(
        exact_k
            .outputs
            .iter()
            .map(|record| record.id.get())
            .collect::<Vec<_>>(),
        vec![101, 3_003, 7_007, 9_001]
    );
    assert_eq!(exact_k.fetch.selected_physical_indices, vec![0, 1, 3, 4]);
    assert_eq!(exact_k.fetch.planned_physical_indices, vec![0, 1, 3, 4]);
    assert_eq!(exact_k.fetch.dummy_flags, vec![false; 4]);
    assert_eq!(exact_k.fetch.selected_real_count, 4);
    assert_eq!(exact_k.fetch.dummy_count, 0);
    assert_eq!(exact_k.fetch_count, 1);
    assert!(exact_k.exact_selection_agreement);
    assert_eq!(exact_k.ckks_metrics["openfhe_version"], "1.5.1");
    assert!(
        exact_k.timings.request_ns
            >= exact_k.timings.match_ns + exact_k.timings.locate_ns + exact_k.timings.fetch_ns
    );
    for output in &exact_k.outputs {
        assert_eq!(output.payload, expected_payload(output.id.get()));
    }

    let partial = pipeline.run(&fps(b"solo")).unwrap();
    assert_eq!(
        partial
            .outputs
            .iter()
            .map(|record| record.id.get())
            .collect::<Vec<_>>(),
        vec![9_001]
    );
    assert_eq!(partial.fetch.selected_physical_indices, vec![4]);
    assert_eq!(partial.fetch.planned_physical_indices, vec![4, 5, 6, 7]);
    assert_eq!(partial.fetch.dummy_flags, vec![false, true, true, true]);
    assert_eq!(partial.fetch.selected_real_count, 1);
    assert_eq!(partial.fetch.dummy_count, 3);
    assert!(partial.exact_selection_agreement);

    let empty = pipeline.run(&fps(b"absent")).unwrap();
    assert!(empty.outputs.is_empty());
    assert!(empty.fetch.selected_physical_indices.is_empty());
    assert_eq!(empty.fetch.planned_physical_indices, vec![5, 6, 7, 8]);
    assert_eq!(empty.fetch.dummy_flags, vec![true; 4]);
    assert_eq!(empty.fetch.selected_real_count, 0);
    assert_eq!(empty.fetch.dummy_count, 4);
    assert_eq!(empty.fetch_count, 1);
    assert!(empty.exact_selection_agreement);
    assert!(empty.fetch.unselected_windows_zero);
}

#[test]
fn every_multi_capacity_uses_one_fetch_for_an_empty_real_selection() {
    for public_k in [4, 8, 16] {
        let records = vec![record(900, b"alpha"), record(100, b"beta")];
        let mut pipeline = MultiPipeline::prepare(records, public_k).unwrap();
        let run = pipeline.run(&fps(b"absent")).unwrap();

        assert_eq!(run.public_k, public_k);
        assert!(run.outputs.is_empty());
        assert_eq!(run.fetch.selected_real_count, 0);
        assert_eq!(run.fetch.dummy_count, public_k);
        assert_eq!(run.fetch.planned_physical_indices.len(), public_k);
        assert!(run.fetch.dummy_flags.iter().all(|is_dummy| *is_dummy));
        assert!(run.fetch.unselected_windows_zero);
        assert_eq!(run.fetch_count, 1);
    }
}

#[test]
fn k4_pipeline_rejects_catalog_that_leaves_too_few_dummy_windows() {
    let records = (1..=61)
        .map(|id| record(id, &[u8::try_from(id).unwrap()]))
        .collect();
    assert!(MultiPipeline::prepare(records, 4).is_err());
}
