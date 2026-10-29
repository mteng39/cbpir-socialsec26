//! Input contract for the frozen realistic K=1 study.
use crate::{LogicalRecord, MatchMode, MatchRequest, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const BGL_SCHEMA: &str = "p0d-bgl-k1-study-v1";
pub const RESULT_SCHEMA: &str = "p0d-bgl-k1-native-result-v1";
pub const EXPECTED_ATTEMPTS: usize = 643;
pub const EXPECTED_RECORD_IDS: [u64; 24] = [
    146, 160, 167, 189, 212, 218, 317, 401, 409, 486, 560, 567, 704, 871, 883, 910, 915, 1037,
    1117, 1180, 1565, 1622, 1660, 1703,
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InputRecord {
    pub id: u64,
    pub searchable: Vec<u8>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogInput {
    pub schema: String,
    pub records: Vec<InputRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CaseInput {
    pub schema: String,
    pub id: String,
    pub pattern_id: String,
    pub pattern_kind: String,
    pub evidence_class: String,
    pub pattern: Vec<u8>,
    pub mode: String,
    pub weight_policy: String,
    pub weight_draw: Option<usize>,
    pub weights: Vec<u8>,
    pub threshold: f64,
    pub oracle: Value,
}

pub fn parse_catalog(raw: &str) -> Result<CatalogInput, String> {
    serde_json::from_str(raw).map_err(|error| format!("catalog JSON: {error}"))
}

pub fn parse_case(raw: &str) -> Result<CaseInput, String> {
    serde_json::from_str(raw).map_err(|error| format!("case JSON: {error}"))
}

pub fn parse_cases(raw: &str) -> Result<Vec<CaseInput>, String> {
    raw.lines()
        .enumerate()
        .map(|(index, line)| {
            if line.is_empty() {
                return Err(format!("empty case line {}", index + 1));
            }
            parse_case(line).map_err(|error| format!("line {}: {error}", index + 1))
        })
        .collect()
}

pub fn logical_records(catalog: CatalogInput) -> Result<Vec<LogicalRecord>, String> {
    if catalog.schema != BGL_SCHEMA {
        return Err("wrong catalog schema".to_owned());
    }
    catalog
        .records
        .into_iter()
        .map(|record| {
            Ok(LogicalRecord {
                id: RecordId::new(record.id).map_err(|error| error.to_string())?,
                searchable: record.searchable,
                payload: record.payload,
            })
        })
        .collect()
}

pub fn request(case: &CaseInput) -> Result<MatchRequest, String> {
    if case.schema != BGL_SCHEMA {
        return Err("wrong case schema".to_owned());
    }
    if case.id.is_empty() || case.pattern_id.is_empty() || case.pattern.is_empty() {
        return Err("empty case ID, pattern ID, or pattern".to_owned());
    }
    if case.pattern.len() > 64 || !case.threshold.is_finite() || case.threshold < 0.0 {
        return Err("invalid pattern length or threshold".to_owned());
    }
    let mode = match case.mode.as_str() {
        "fps"
            if case.weight_policy == "none"
                && case.weight_draw.is_none()
                && case.weights.is_empty()
                && case.threshold.to_bits() == 0.5_f64.to_bits() =>
        {
            MatchMode::Fps
        }
        "rfps"
            if matches!(
                case.weight_policy.as_str(),
                "uniform_3_32" | "ones_diagnostic"
            ) && case.weight_draw.is_some()
                && case.weights.len() == case.pattern.len()
                && case.weights.iter().all(|weight| (1..=32).contains(weight)) =>
        {
            MatchMode::Rfps {
                randomizer: case.weights.clone(),
            }
        }
        _ => return Err("mode, weight policy, weights, or FPS threshold drift".to_owned()),
    };
    if case.weight_policy == "uniform_3_32"
        && case.weights.iter().any(|weight| !(3..=32).contains(weight))
    {
        return Err("uniform RFPS weight outside 3..=32".to_owned());
    }
    if case.weight_policy == "ones_diagnostic"
        && (case.weights.iter().any(|weight| *weight != 1)
            || !matches!(case.pattern_kind.as_str(), "one_change" | "balanced_change"))
    {
        return Err("all-one policy used outside controlled diagnostics".to_owned());
    }
    Ok(MatchRequest {
        pattern: case.pattern.clone(),
        threshold: case.threshold,
        mode,
    })
}

pub fn validate_fixed_catalog(catalog: &CatalogInput) -> Result<(), String> {
    if catalog.schema != BGL_SCHEMA {
        return Err("wrong catalog schema".to_owned());
    }
    let ids: Vec<_> = catalog.records.iter().map(|record| record.id).collect();
    if ids != EXPECTED_RECORD_IDS {
        return Err("selected RecordId list drift".to_owned());
    }
    if catalog.records.iter().any(|record| {
        record.searchable != record.payload || !(16..=40).contains(&record.payload.len())
    }) {
        return Err("record layout or payload drift".to_owned());
    }
    let payload_bytes: usize = catalog
        .records
        .iter()
        .map(|record| record.payload.len())
        .sum();
    if payload_bytes != 595 || payload_bytes + catalog.records.len() - 1 != 618 {
        return Err("catalog byte totals drift".to_owned());
    }
    Ok(())
}

pub fn validate_fixed_cases(cases: &[CaseInput]) -> Result<(), String> {
    if cases.len() != EXPECTED_ATTEMPTS {
        return Err(format!(
            "expected {EXPECTED_ATTEMPTS} cases, got {}",
            cases.len()
        ));
    }
    let mut ids = HashSet::new();
    let mut patterns = HashSet::new();
    let mut fps = 0;
    let mut uniform = 0;
    let mut ones = 0;
    let mut paired: HashMap<(String, usize), (Vec<u8>, Vec<f64>)> = HashMap::new();
    for case in cases {
        request(case)?;
        if !ids.insert(case.id.clone()) {
            return Err("duplicate case ID".to_owned());
        }
        patterns.insert(case.pattern_id.clone());
        match case.weight_policy.as_str() {
            "none" => fps += 1,
            "uniform_3_32" => {
                uniform += 1;
                let key = (
                    case.pattern_id.clone(),
                    case.weight_draw.unwrap_or_default(),
                );
                let entry = paired
                    .entry(key)
                    .or_insert_with(|| (case.weights.clone(), Vec::new()));
                if entry.0 != case.weights {
                    return Err("threshold-paired weights drift".to_owned());
                }
                entry.1.push(case.threshold);
            }
            "ones_diagnostic" => ones += 1,
            _ => return Err("unknown weight policy".to_owned()),
        }
    }
    if patterns.len() != 13 || fps != 13 || uniform != 624 || ones != 6 || paired.len() != 208 {
        return Err("fixed matrix denominator drift".to_owned());
    }
    for (_, thresholds) in paired.values_mut() {
        thresholds.sort_by(f64::total_cmp);
        if thresholds != &[0.5, 1.0, 4.0] {
            return Err("threshold pairing drift".to_owned());
        }
    }
    Ok(())
}
