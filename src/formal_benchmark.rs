//! Thin fixed-window Fetch stages for the formal CBPIR benchmark.
//!
//! Context/key construction is intentionally separate from database Setup.

use crate::{
    ALLOWED_FIXED_WINDOW_SLOTS, FRAME_LEN, LogicalRecord, P0dError, ReconstructedRecord, RecordId,
    decode_frame, encode_frame, inspect_fixed_frame,
};
use cbpir_bgv::{
    ArtifactSizes, BgvCpir, BgvProfile, EncryptedQuery, EncryptedResponse, PreparedDatabase,
    SetupTimings,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
pub struct FormalFetchRuntimeProfile {
    pub profile: &'static str,
    pub security: &'static str,
    pub requested_ring_dimension: usize,
    pub actual_ring_dimension: usize,
    pub plaintext_modulus: u64,
    pub multiplicative_depth: u32,
    pub record_slots: usize,
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FormalFetchMessageSizes {
    pub query_bytes: u64,
    pub response_bytes: u64,
}

pub struct FormalFetchDatabase {
    database: PreparedDatabase,
    ids: Vec<RecordId>,
    id_to_index: BTreeMap<RecordId, usize>,
    packed_records: Vec<Vec<u8>>,
    record_slots: usize,
    capacity: usize,
}

impl FormalFetchDatabase {
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn record_slots(&self) -> usize {
        self.record_slots
    }

    #[must_use]
    pub fn ids(&self) -> &[RecordId] {
        &self.ids
    }

    #[must_use]
    pub fn packed_records(&self) -> &[Vec<u8>] {
        &self.packed_records
    }
}

pub struct FormalFetchQuery {
    query: EncryptedQuery,
    selected_ids: Vec<RecordId>,
    selected_indices: Vec<usize>,
}

impl FormalFetchQuery {
    #[must_use]
    pub const fn ciphertext_count(&self) -> usize {
        1
    }

    #[must_use]
    pub fn selected_ids(&self) -> &[RecordId] {
        &self.selected_ids
    }

    #[must_use]
    pub fn selected_indices(&self) -> &[usize] {
        &self.selected_indices
    }
}

pub struct FormalFetchResponse {
    response: EncryptedResponse,
}

impl FormalFetchResponse {
    #[must_use]
    pub const fn ciphertext_count(&self) -> usize {
        1
    }
}

#[derive(Clone, Debug)]
pub struct FormalFetchRecovered {
    pub records: Vec<ReconstructedRecord>,
    pub slots: Vec<u64>,
}

pub struct FormalFetchContext {
    kernel: BgvCpir,
    runtime: FormalFetchRuntimeProfile,
}

impl FormalFetchContext {
    pub fn new(profile: BgvProfile, record_slots: usize) -> Result<Self, P0dError> {
        let mut config = profile.config();
        config.record_slots = record_slots;
        Self::from_config(profile.label(), config)
    }

    /// Builds Fetch from an explicit BGV configuration for scaling studies.
    pub fn from_config(
        profile: &'static str,
        config: cbpir_bgv::BgvCpirConfig,
    ) -> Result<Self, P0dError> {
        let record_slots = config.record_slots;
        if !ALLOWED_FIXED_WINDOW_SLOTS.contains(&record_slots) {
            return Err(P0dError::FetchComponent(format!(
                "record width {record_slots} is outside the allowed fixed-window set"
            )));
        }
        let (
            kernel,
            SetupTimings {
                context_generation_ns,
                key_generation_ns,
            },
        ) = BgvCpir::new_with_setup_timings(config)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let runtime = FormalFetchRuntimeProfile {
            profile,
            security: "HEStd_128_classic",
            requested_ring_dimension: config.ring_dimension,
            actual_ring_dimension: kernel.actual_ring_dimension(),
            plaintext_modulus: kernel.plaintext_modulus(),
            multiplicative_depth: config.multiplicative_depth,
            record_slots,
            context_generation_ns,
            key_generation_ns,
        };
        Ok(Self { kernel, runtime })
    }

    #[must_use]
    pub const fn runtime_profile(&self) -> &FormalFetchRuntimeProfile {
        &self.runtime
    }

    pub fn prepare_database(
        &self,
        records: &[LogicalRecord],
    ) -> Result<FormalFetchDatabase, P0dError> {
        let mut packed_records = Vec::with_capacity(records.len());
        for record in records {
            let inspection = inspect_fixed_frame(record)?;
            if inspection.required_slots > self.runtime.record_slots {
                return Err(P0dError::FetchComponent(format!(
                    "record {} requires {} slots but fixed width is {}",
                    record.id.get(),
                    inspection.required_slots,
                    self.runtime.record_slots
                )));
            }
            let frame = encode_frame(record)?;
            let mut packed = vec![0_u8; self.runtime.record_slots];
            let copied = FRAME_LEN.min(self.runtime.record_slots);
            packed[..copied].copy_from_slice(&frame[..copied]);
            packed_records.push(packed);
        }
        let database = self
            .kernel
            .prepare_database(&packed_records)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let id_to_index = ids
            .iter()
            .copied()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        Ok(FormalFetchDatabase {
            database,
            ids,
            id_to_index,
            packed_records,
            record_slots: self.runtime.record_slots,
            capacity: self.kernel.capacity(),
        })
    }

    pub fn fetch_query(
        &self,
        database: &FormalFetchDatabase,
        selected_ids: &[RecordId],
    ) -> Result<FormalFetchQuery, P0dError> {
        let selected_indices = selected_ids
            .iter()
            .map(|id| {
                database.id_to_index.get(id).copied().ok_or_else(|| {
                    P0dError::FetchComponent(format!("unknown selected RecordId {}", id.get()))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let query = self
            .kernel
            .encrypt_window_query(&selected_indices)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        Ok(FormalFetchQuery {
            query,
            selected_ids: selected_ids.to_vec(),
            selected_indices,
        })
    }

    #[must_use]
    pub fn fetch_response(
        &self,
        database: &mut FormalFetchDatabase,
        query: &FormalFetchQuery,
    ) -> FormalFetchResponse {
        FormalFetchResponse {
            response: self.kernel.answer(&mut database.database, &query.query),
        }
    }

    pub fn recover(
        &self,
        database: &FormalFetchDatabase,
        query: &FormalFetchQuery,
        response: &FormalFetchResponse,
    ) -> Result<FormalFetchRecovered, P0dError> {
        let slots = self
            .kernel
            .decrypt_slots(&response.response)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let records = query
            .selected_indices
            .iter()
            .copied()
            .zip(query.selected_ids.iter().copied())
            .map(|(index, id)| {
                let recovered = self
                    .kernel
                    .decode_fixed_window(&slots, index, database.record_slots)
                    .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
                if recovered[FRAME_LEN.min(database.record_slots)..]
                    .iter()
                    .any(|value| *value != 0)
                {
                    return Err(P0dError::Frame("nonzero slots beyond legacy frame"));
                }
                let mut frame = [0_u8; FRAME_LEN];
                let copied = FRAME_LEN.min(database.record_slots);
                frame[..copied].copy_from_slice(&recovered[..copied]);
                decode_frame(&frame, id)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(FormalFetchRecovered { records, slots })
    }

    pub fn serialize_messages(
        &self,
        directory: &Path,
        query: &FormalFetchQuery,
        response: &FormalFetchResponse,
    ) -> Result<FormalFetchMessageSizes, P0dError> {
        let ArtifactSizes {
            query_bytes,
            response_bytes,
            ..
        } = self
            .kernel
            .serialize_binary_bundle(directory, &query.query, &response.response)
            .map_err(|error| P0dError::Serialization(error.to_string()))?
            .sizes;
        Ok(FormalFetchMessageSizes {
            query_bytes,
            response_bytes,
        })
    }
}
