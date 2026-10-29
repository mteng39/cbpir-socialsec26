//! Audit the meaningful prefix of the retained fixed 128-byte Fetch frame.

use cbpir::{LogicalRecord, RecordId, inspect_fixed_frame};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Clone, Debug, Deserialize)]
struct InputRecord {
    id: u64,
    searchable: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
struct AuditRow {
    id: u64,
    searchable_bytes: usize,
    payload_bytes: usize,
    embedded_payload_length: usize,
    required_slots: usize,
    legacy_padding_slots: usize,
    legacy_padding_zero: bool,
    round_trip_exact: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let records: Vec<InputRecord> = serde_json::from_str(&input)?;
    let rows = records
        .into_iter()
        .map(|record| {
            let logical = LogicalRecord {
                id: RecordId::new(record.id)?,
                searchable: record.searchable,
                payload: record.payload,
            };
            let inspection = inspect_fixed_frame(&logical)?;
            Ok(AuditRow {
                id: logical.id.get(),
                searchable_bytes: logical.searchable.len(),
                payload_bytes: logical.payload.len(),
                embedded_payload_length: inspection.payload_length,
                required_slots: inspection.required_slots,
                legacy_padding_slots: inspection.legacy_padding_slots,
                legacy_padding_zero: inspection.legacy_padding_zero,
                round_trip_exact: inspection.round_trip_exact,
            })
        })
        .collect::<Result<Vec<_>, cbpir::P0dError>>()?;
    serde_json::to_writer(io::stdout().lock(), &rows)?;
    Ok(())
}
