#![cfg(feature = "native")]

use cbpir::{ALLOWED_FIXED_WINDOW_SLOTS, ConfigurableFixedFetch, LogicalRecord, RecordId};
use cbpir_bgv::BgvProfile;

fn records(count: usize) -> Vec<LogicalRecord> {
    (0..count)
        .map(|index| LogicalRecord {
            id: RecordId::new(50_000 + index as u64).unwrap(),
            searchable: format!("search-{index:03}").into_bytes(),
            payload: vec![b'A' + u8::try_from(index % 20).unwrap(); 16 + index % 25],
        })
        .collect()
}

#[test]
fn measured_frame_prefix_is_header_plus_payload_for_every_allowed_width() {
    let records = records(3);
    for width in ALLOWED_FIXED_WINDOW_SLOTS {
        let fetch = ConfigurableFixedFetch::prepare(&records, BgvProfile::Bgv13, width).unwrap();
        assert_eq!(fetch.preparation().record_slots, width);
        assert_eq!(fetch.preparation().physical_dummy_window_count, 0);
        for (entry, record) in fetch.layout().iter().zip(&records) {
            assert_eq!(entry.id, record.id);
            assert_eq!(entry.required_slots, 14 + record.payload.len());
            assert_eq!(entry.window_start, entry.physical_index * width);
            assert_eq!(entry.window_end, entry.window_start + width);
        }
    }
}

#[test]
fn configurable_fixed_fetch_rejects_nonpolicy_width_and_too_small_window() {
    let records = vec![LogicalRecord {
        id: RecordId::new(9).unwrap(),
        searchable: b"search".to_vec(),
        payload: vec![7; 114],
    }];

    assert!(
        ConfigurableFixedFetch::prepare(&records, BgvProfile::Bgv13, 63)
            .err()
            .unwrap()
            .to_string()
            .contains("allowed fixed-window set")
    );
    assert!(
        ConfigurableFixedFetch::prepare(&records, BgvProfile::Bgv13, 64)
            .err()
            .unwrap()
            .to_string()
            .contains("requires 128 slots")
    );
}

#[test]
fn fixed_fetch_preserves_one_ciphertext_shape_for_k_zero_through_seventeen() {
    let records = records(20);
    let mut fetch = ConfigurableFixedFetch::prepare(&records, BgvProfile::Bgv13, 64).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut query_sizes = Vec::new();
    let mut response_sizes = Vec::new();

    for selected_size in 0..=17 {
        let selected = (0..selected_size).collect::<Vec<_>>();
        let trace = fetch
            .run(&selected, &directory.path().join(selected_size.to_string()))
            .unwrap();
        assert_eq!(trace.query_ciphertext_count, 1);
        assert_eq!(trace.response_ciphertext_count, 1);
        assert_eq!(trace.outputs.len(), selected_size);
        assert!(trace.selected_payloads_exact);
        assert!(trace.unselected_windows_zero);
        assert!(trace.unused_tail_zero);
        assert_eq!(trace.empty_response_all_zero, selected_size == 0);
        query_sizes.push(trace.query_bytes);
        response_sizes.push(trace.response_bytes);
    }

    assert!(query_sizes.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(response_sizes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn first_last_and_different_size_fixed_windows_round_trip() {
    let records = records(20);
    let mut fetch = ConfigurableFixedFetch::prepare(&records, BgvProfile::Bgv13, 224).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let selected = [0, 1, records.len() - 1];

    let trace = fetch.run(&selected, directory.path()).unwrap();

    assert_eq!(
        trace
            .outputs
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        selected
            .iter()
            .map(|&index| records[index].id)
            .collect::<Vec<_>>()
    );
    assert!(trace.selected_payloads_exact);
    assert!(trace.unselected_windows_zero);
    assert!(trace.unused_tail_zero);
}
