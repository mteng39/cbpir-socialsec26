use crate::CliConfig;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const BGL_BYTES: &[u8] = include_bytes!("../data/source/loghub/BGL_2k.log");
const EXPECTED_BGL_SHA256: &str =
    "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496";

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
#[error("{0}")]
pub struct WorkloadError(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkloadRecord {
    pub id: u64,
    pub source_line: usize,
    pub searchable: Vec<u8>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PatternFixture {
    pub bytes: Vec<u8>,
    pub support: usize,
    pub literal_alignment_positions: Vec<usize>,
    pub literal_record_ids: Vec<u64>,
    pub first_record_index: usize,
    pub first_local_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GeneratedWorkload {
    pub dataset: &'static str,
    pub source_sha256: String,
    pub source_line_count: usize,
    pub eligible_record_count: usize,
    pub records: Vec<WorkloadRecord>,
    pub pattern: PatternFixture,
}

#[derive(Debug)]
struct SourceRow<'a> {
    line_number: usize,
    message: &'a [u8],
}

#[derive(Debug)]
struct PatternCandidate {
    alignment_positions: Vec<usize>,
    record_ids: BTreeSet<u64>,
    first_record_index: usize,
    first_local_offset: usize,
}

pub fn resize_by_repetition(source: &[u8], length: usize) -> Result<Vec<u8>, WorkloadError> {
    if source.is_empty() {
        return Err(WorkloadError("cannot resize an empty source field".into()));
    }
    Ok(source.iter().copied().cycle().take(length).collect())
}

fn message_field(line: &[u8]) -> Option<&[u8]> {
    let mut index = 0;
    for _ in 0..9 {
        while index < line.len() && line[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < line.len() && !line[index].is_ascii_whitespace() {
            index += 1;
        }
        if start == index {
            return None;
        }
    }
    while index < line.len() && line[index].is_ascii_whitespace() {
        index += 1;
    }
    (index < line.len()).then_some(&line[index..])
}

fn source_rows() -> Vec<SourceRow<'static>> {
    BGL_BYTES
        .split(|byte| *byte == b'\n')
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let message = message_field(line)?;
            (message.len() >= 16
                && message.len() <= 40
                && message.is_ascii()
                && !message.contains(&0))
            .then_some(SourceRow {
                line_number: index + 1,
                message,
            })
        })
        .collect()
}

fn select_pattern(
    records: &[WorkloadRecord],
    pattern_length: usize,
    requested_support: usize,
) -> Result<PatternFixture, WorkloadError> {
    let mut candidates = BTreeMap::<Vec<u8>, PatternCandidate>::new();
    let mut global_start = 0;
    for (record_index, record) in records.iter().enumerate() {
        for local_offset in 0..=record.searchable.len() - pattern_length {
            let bytes = record.searchable[local_offset..local_offset + pattern_length].to_vec();
            let candidate = candidates
                .entry(bytes)
                .or_insert_with(|| PatternCandidate {
                    alignment_positions: Vec::new(),
                    record_ids: BTreeSet::new(),
                    first_record_index: record_index,
                    first_local_offset: local_offset,
                });
            candidate
                .alignment_positions
                .push(global_start + local_offset);
            candidate.record_ids.insert(record.id);
        }
        global_start += record.searchable.len() + 1;
    }
    let (bytes, candidate) = candidates
        .into_iter()
        .min_by_key(|(bytes, candidate)| {
            (
                candidate.record_ids.len().abs_diff(requested_support),
                candidate.first_record_index,
                candidate.first_local_offset,
                bytes.clone(),
            )
        })
        .ok_or_else(|| WorkloadError("generated workload contains no pattern candidate".into()))?;
    let literal_record_ids = candidate.record_ids.into_iter().collect::<Vec<_>>();
    Ok(PatternFixture {
        bytes,
        support: literal_record_ids.len(),
        literal_alignment_positions: candidate.alignment_positions,
        literal_record_ids,
        first_record_index: candidate.first_record_index,
        first_local_offset: candidate.first_local_offset,
    })
}

pub fn build_bgl_workload(config: &CliConfig) -> Result<GeneratedWorkload, WorkloadError> {
    let source_sha256 = format!("{:x}", Sha256::digest(BGL_BYTES));
    if source_sha256 != EXPECTED_BGL_SHA256 {
        return Err(WorkloadError(format!(
            "unexpected BGL SHA-256: {source_sha256}"
        )));
    }
    let rows = source_rows();
    if rows.len() < config.num_records {
        return Err(WorkloadError(format!(
            "requested {} records but pinned BGL fixture has {} eligible records",
            config.num_records,
            rows.len()
        )));
    }
    let records = rows
        .iter()
        .take(config.num_records)
        .map(|row| {
            Ok(WorkloadRecord {
                id: u64::try_from(row.line_number)
                    .map_err(|_| WorkloadError("source line exceeds u64".into()))?,
                source_line: row.line_number,
                searchable: resize_by_repetition(row.message, config.searchable_bytes)?,
                payload: resize_by_repetition(row.message, config.payload_bytes)?,
            })
        })
        .collect::<Result<Vec<_>, WorkloadError>>()?;
    let pattern = select_pattern(&records, config.pattern_length, config.capacity)?;
    Ok(GeneratedWorkload {
        dataset: "bgl",
        source_sha256,
        source_line_count: BGL_BYTES
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        eligible_record_count: rows.len(),
        records,
        pattern,
    })
}
