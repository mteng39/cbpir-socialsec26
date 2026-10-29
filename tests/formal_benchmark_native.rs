#![cfg(feature = "native")]

use cbpir::{
    Catalog, FormalFetchContext, LogicalRecord, MatchMode, MatchRequest, RecordId,
    locate_many_from_scores,
};
use cbpir_bgv::BgvProfile;
use cbpir_search::{FpsProfile, ServerTextCtPtFpsStaged};

fn records(count: usize) -> Vec<LogicalRecord> {
    (0..count)
        .map(|index| LogicalRecord {
            id: RecordId::new(u64::try_from(index + 1).unwrap()).unwrap(),
            searchable: vec![b'x'],
            payload: vec![u8::try_from(index + 1).unwrap()],
        })
        .collect()
}

#[test]
fn formal_stages_map_stable_ids_and_preserve_fixed_window_fetch() {
    let catalog = Catalog::new(records(17)).unwrap();
    let request = MatchRequest {
        pattern: vec![b'x'],
        threshold: 0.5,
        mode: MatchMode::Fps,
    };
    let transform_len = (catalog.text().len() + 1).next_power_of_two();
    let match_context =
        ServerTextCtPtFpsStaged::new(FpsProfile::Fps13, transform_len, request.threshold).unwrap();
    let mut search_view = match_context.prepare_server(catalog.text()).unwrap();
    let query = match_context
        .match_query(catalog.text().len(), &request.pattern)
        .unwrap();
    let response = match_context
        .match_response(&mut search_view, &query)
        .unwrap();
    let scores = match_context
        .decrypt_idft_scores(&response, catalog.text().len(), request.pattern.len())
        .unwrap();
    let located = locate_many_from_scores(&catalog, &request, &scores, 16).unwrap();
    assert_eq!(located.candidates.len(), 17);
    assert_eq!(located.selected_candidates.len(), 16);
    assert_eq!(
        located
            .selected_candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>(),
        catalog
            .records()
            .iter()
            .take(16)
            .map(|record| record.id)
            .collect::<Vec<_>>()
    );

    let fetch_context = FormalFetchContext::new(BgvProfile::Bgv13, 64).unwrap();
    let mut fetch_view = fetch_context.prepare_database(catalog.records()).unwrap();
    for selected_size in [0, 1, 4, 8, 16] {
        let selected_ids = located
            .candidates
            .iter()
            .take(selected_size)
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        let fetch_query = fetch_context
            .fetch_query(&fetch_view, &selected_ids)
            .unwrap();
        let fetch_response = fetch_context.fetch_response(&mut fetch_view, &fetch_query);
        let output = fetch_context
            .recover(&fetch_view, &fetch_query, &fetch_response)
            .unwrap();
        assert_eq!(fetch_query.ciphertext_count(), 1);
        assert_eq!(fetch_response.ciphertext_count(), 1);
        assert_eq!(output.records.len(), selected_size);
        assert!(
            output
                .records
                .iter()
                .zip(selected_ids.iter())
                .all(|(record, id)| record.id == *id)
        );
        for index in 0..fetch_view.capacity() {
            let start = index * 64;
            let end = start + 64;
            if !fetch_query.selected_indices().contains(&index) {
                assert!(output.slots[start..end].iter().all(|value| *value == 0));
            }
        }
        if selected_size == 0 {
            assert!(output.slots.iter().all(|value| *value == 0));
        }
    }
}
