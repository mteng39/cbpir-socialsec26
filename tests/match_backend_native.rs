#![cfg(feature = "native")]

use cbpir::{
    LogicalRecord, MatchBackend, MatchMode, MatchRequest, MultiPipeline, Pipeline, RecordId,
};

fn record(id: u64, searchable: &[u8]) -> LogicalRecord {
    LogicalRecord {
        id: RecordId::new(id).unwrap(),
        searchable: searchable.to_vec(),
        payload: format!("payload-{id}").into_bytes(),
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
fn k1_default_stays_ct_ct_and_explicit_server_text_ct_pt_reconstructs_exactly() {
    let records = vec![record(9001, b"alpha"), record(101, b"needle")];

    let mut baseline = Pipeline::prepare(records.clone()).unwrap();
    let baseline_run = baseline.run(&fps(b"needle")).unwrap();
    assert_eq!(baseline_run.match_backend, MatchBackend::CtCt);
    assert_eq!(baseline_run.ckks_metrics["backend"], "ct_ct");

    let mut variant =
        Pipeline::prepare_with_match_backend(records, MatchBackend::ServerTextCtPt).unwrap();
    let variant_run = variant.run(&fps(b"needle")).unwrap();
    assert_eq!(variant_run.match_backend, MatchBackend::ServerTextCtPt);
    assert_eq!(variant_run.ckks_metrics["backend"], "server_text_ct_pt");
    assert_eq!(
        variant_run.ckks_metrics["operations"]["ciphertext_multiplications"],
        0
    );
    assert_eq!(variant_run.output.unwrap().id.get(), 101);
    assert!(variant_run.exact_selection_agreement);
}

#[test]
fn k4_explicit_server_text_ct_pt_keeps_locate_and_p3_fetch_contracts() {
    let records = vec![
        record(9001, b"common"),
        record(101, b"common"),
        record(7007, b"common"),
        record(3003, b"common"),
        record(5005, b"other"),
    ];
    let mut pipeline =
        MultiPipeline::prepare_with_match_backend(records, 4, MatchBackend::ServerTextCtPt)
            .unwrap();
    let run = pipeline.run(&fps(b"common")).unwrap();

    assert_eq!(run.match_backend, MatchBackend::ServerTextCtPt);
    assert_eq!(run.ckks_metrics["backend"], "server_text_ct_pt");
    assert_eq!(run.outputs.len(), 4);
    assert!(run.fetch.unselected_windows_zero);
    assert_eq!(run.fetch_count, 1);
    assert!(run.exact_selection_agreement);
}
