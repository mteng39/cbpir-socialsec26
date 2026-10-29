#![cfg(feature = "native")]

use cbpir::{MultiPipeline, realistic, realistic_kgt1};

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/fixtures/bgl-k1-study/catalog.json"
));
const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/fixtures/bgl-k1-study/cases.jsonl"
));

#[test]
fn native_row_retains_a_realistic_multi_record_result() {
    let catalog_input = realistic::parse_catalog(CATALOG).unwrap();
    let records = realistic::logical_records(catalog_input).unwrap();
    let cases = realistic::parse_cases(CASES).unwrap();
    let case = cases
        .iter()
        .find(|case| {
            case.mode == "fps"
                && case.oracle["literal_record_ids"]
                    .as_array()
                    .is_some_and(|ids| ids.len() > 1)
        })
        .unwrap();
    let expected = realistic_kgt1::expected(case, 4).unwrap();
    let mut pipeline = MultiPipeline::prepare(records, 4).unwrap();

    let row = realistic_kgt1::run_case(&mut pipeline, case);

    assert_eq!(row["schema"], realistic_kgt1::RESULT_SCHEMA);
    assert!(row["error"].is_null());
    let selected = row["run"]["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["id"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(selected, expected.selected_ids);
    assert!(selected.len() > 1);
    assert_eq!(row["run"]["fetch_count"], 1);
    assert_eq!(row["run"]["fetch"]["unselected_windows_zero"], true);
}
