use crate::{LogicalRecord, MatchMode, MatchRequest, RecordId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct MultiFixture {
    pub name: String,
    pub public_k: usize,
    pub records: Vec<LogicalRecord>,
    pub request: MatchRequest,
    pub expected_candidate_ids: Vec<u64>,
    pub expected_selected_ids: Vec<u64>,
    pub expected_semantic_ids: Vec<u64>,
    pub expected_boundary_rejections: usize,
    pub expected_duplicate_count: usize,
    pub expected_truncated_count: usize,
}

fn record(id: u64, searchable: &[u8]) -> LogicalRecord {
    let id_bytes = id.to_le_bytes();
    LogicalRecord {
        id: RecordId::new(id).expect("synthetic fixture ID is nonzero"),
        searchable: searchable.to_vec(),
        payload: vec![0, id_bytes[0], 255, id_bytes[1], 0],
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the frozen fixture row keeps each expected decision explicit"
)]
fn fps(
    name: &str,
    records: Vec<LogicalRecord>,
    pattern: &[u8],
    candidates: &[u64],
    selected: &[u64],
    boundary_rejections: usize,
    duplicates: usize,
    truncated: usize,
) -> MultiFixture {
    MultiFixture {
        name: name.into(),
        public_k: 4,
        records,
        request: MatchRequest {
            pattern: pattern.to_vec(),
            threshold: 0.5,
            mode: MatchMode::Fps,
        },
        expected_candidate_ids: candidates.to_vec(),
        expected_selected_ids: selected.to_vec(),
        expected_semantic_ids: candidates.to_vec(),
        expected_boundary_rejections: boundary_rejections,
        expected_duplicate_count: duplicates,
        expected_truncated_count: truncated,
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the frozen fixture row keeps each expected decision explicit"
)]
fn rfps(
    name: &str,
    records: Vec<LogicalRecord>,
    pattern: &[u8],
    weights: &[u8],
    threshold: f64,
    candidates: &[u64],
    selected: &[u64],
    semantic: &[u64],
    truncated: usize,
) -> MultiFixture {
    MultiFixture {
        name: name.into(),
        public_k: 4,
        records,
        request: MatchRequest {
            pattern: pattern.to_vec(),
            threshold,
            mode: MatchMode::Rfps {
                randomizer: weights.to_vec(),
            },
        },
        expected_candidate_ids: candidates.to_vec(),
        expected_selected_ids: selected.to_vec(),
        expected_semantic_ids: semantic.to_vec(),
        expected_boundary_rejections: 0,
        expected_duplicate_count: 0,
        expected_truncated_count: truncated,
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one contiguous table keeps the frozen 14-case matrix auditable"
)]
pub fn multi_fixtures() -> Vec<MultiFixture> {
    vec![
        fps(
            "zero_matches",
            vec![record(10, b"alpha"), record(20, b"beta")],
            b"zzz",
            &[],
            &[],
            0,
            0,
            0,
        ),
        fps(
            "one_record",
            vec![record(20, b"needle"), record(10, b"other")],
            b"needle",
            &[20],
            &[20],
            0,
            0,
            0,
        ),
        fps(
            "fewer_than_k",
            vec![
                record(30, b"needle"),
                record(10, b"needle"),
                record(20, b"otherx"),
            ],
            b"needle",
            &[10, 30],
            &[10, 30],
            0,
            0,
            0,
        ),
        fps(
            "exactly_k",
            vec![
                record(40, b"tag"),
                record(10, b"tag"),
                record(30, b"tag"),
                record(20, b"tag"),
            ],
            b"tag",
            &[10, 20, 30, 40],
            &[10, 20, 30, 40],
            0,
            0,
            0,
        ),
        fps(
            "more_than_k",
            vec![
                record(50, b"tag"),
                record(10, b"tag"),
                record(40, b"tag"),
                record(20, b"tag"),
                record(30, b"tag"),
            ],
            b"tag",
            &[10, 20, 30, 40, 50],
            &[10, 20, 30, 40],
            0,
            0,
            1,
        ),
        fps(
            "one_record_many_occurrences",
            vec![record(10, b"AAAAA")],
            b"AA",
            &[10],
            &[10],
            0,
            3,
            0,
        ),
        fps(
            "duplicates_across_records",
            vec![record(20, b"AAA"), record(10, b"AAAA")],
            b"AA",
            &[10, 20],
            &[10, 20],
            0,
            3,
            0,
        ),
        fps(
            "adjacent_records",
            vec![
                record(10, b"tag"),
                record(20, b"tag"),
                record(30, b"xxx"),
                record(40, b"yyy"),
            ],
            b"tag",
            &[10, 20],
            &[10, 20],
            0,
            0,
            0,
        ),
        fps(
            "nonadjacent_records",
            vec![
                record(10, b"tag"),
                record(20, b"xxx"),
                record(30, b"tag"),
                record(40, b"yyy"),
            ],
            b"tag",
            &[10, 30],
            &[10, 30],
            0,
            0,
            0,
        ),
        fps(
            "first_and_last_records",
            vec![
                record(10, b"tag"),
                record(20, b"xxx"),
                record(30, b"yyy"),
                record(40, b"tag"),
            ],
            b"tag",
            &[10, 40],
            &[10, 40],
            0,
            0,
            0,
        ),
        fps(
            "cross_record",
            vec![record(10, b"AB"), record(20, b"CD")],
            b"B\0C",
            &[],
            &[],
            1,
            0,
            0,
        ),
        rfps(
            "rfps_false_only_cancellation",
            vec![record(10, &[2, 0])],
            &[1, 1],
            &[3, 3],
            0.5,
            &[10],
            &[10],
            &[],
            0,
        ),
        rfps(
            "rfps_false_competes",
            vec![
                record(5, &[2, 0]),
                record(10, &[1, 1]),
                record(20, &[1, 1]),
                record(30, &[1, 1]),
                record(40, &[1, 1]),
            ],
            &[1, 1],
            &[3, 3],
            0.5,
            &[5, 10, 20, 30, 40],
            &[5, 10, 20, 30],
            &[10, 20, 30, 40],
            1,
        ),
        rfps(
            "rfps_threshold_only",
            vec![record(10, &[2])],
            &[1],
            &[3],
            4.0,
            &[10],
            &[10],
            &[],
            0,
        ),
    ]
}
