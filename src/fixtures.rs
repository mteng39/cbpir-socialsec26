use crate::{LogicalRecord, MatchMode, MatchRequest, RecordId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Fixture {
    pub name: String,
    pub records: Vec<LogicalRecord>,
    pub request: MatchRequest,
    pub expected_id: Option<u64>,
    pub semantic_negative: bool,
}
fn record(id: u64, text: &[u8]) -> LogicalRecord {
    LogicalRecord {
        id: RecordId::new(id).expect("fixture ID nonzero"),
        searchable: text.to_vec(),
        payload: vec![u8::try_from(id).expect("small fixture ID"), 0, 255, 0],
    }
}
fn fps(
    name: &str,
    records: Vec<LogicalRecord>,
    pattern: &[u8],
    expected_id: Option<u64>,
) -> Fixture {
    Fixture {
        name: name.into(),
        records,
        request: MatchRequest {
            pattern: pattern.to_vec(),
            threshold: 0.5,
            mode: MatchMode::Fps,
        },
        expected_id,
        semantic_negative: false,
    }
}
pub fn fixtures() -> Vec<Fixture> {
    let mut cases = vec![
        fps(
            "exact_first",
            vec![record(30, b"later"), record(10, b"needle")],
            b"needle",
            Some(10),
        ),
        fps(
            "exact_last",
            vec![record(30, b"needle"), record(10, b"other")],
            b"needle",
            Some(30),
        ),
        fps("near_mismatch", vec![record(10, &[2])], &[1], None),
        fps("no_match", vec![record(10, b"abcdef")], b"zzz", None),
        fps(
            "ties",
            vec![record(30, b"AA"), record(10, b"AA")],
            b"AA",
            Some(10),
        ),
        fps("duplicates", vec![record(10, b"AAAA")], b"AA", Some(10)),
        fps(
            "cross_record",
            vec![record(10, b"AB"), record(30, b"CD")],
            b"AB\0C",
            None,
        ),
        fps(
            "zero_inside_record",
            vec![record(10, b"A\0B")],
            b"A\0B",
            Some(10),
        ),
    ];
    for (name, text, pattern, randomizer, threshold, negative) in [
        ("rfps_exact", vec![1, 2], vec![1, 2], vec![3, 4], 0.5, false),
        (
            "rfps_cancellation",
            vec![2, 0],
            vec![1, 1],
            vec![1, 1],
            0.5,
            true,
        ),
        ("rfps_threshold_only", vec![2], vec![1], vec![1], 4.0, true),
    ] {
        let mut f = fps(name, vec![record(10, &text)], &pattern, Some(10));
        f.request.mode = MatchMode::Rfps { randomizer };
        f.request.threshold = threshold;
        f.semantic_negative = negative;
        cases.push(f);
    }
    cases
}
