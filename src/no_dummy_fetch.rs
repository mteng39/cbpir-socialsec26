//! Phase-2 BGV Fetch over real record windows only.

use crate::{LogicalRecord, P0dError, ReconstructedRecord, RecordId, decode_frame, encode_frame};
use cbpir_bgv::{BgvCpir, BgvProfile, PreparedDatabase};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, Serialize)]
pub struct NoDummyFetchPreparation {
    pub profile: &'static str,
    pub requested_ring_dimension: usize,
    pub actual_ring_dimension: usize,
    pub record_slots: usize,
    pub real_record_count: usize,
    pub physical_capacity: usize,
    pub physical_dummy_window_count: usize,
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
    pub database_preparation_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NoDummyFetchTimings {
    pub fetch_query_ns: u64,
    pub fetch_response_ns: u64,
    pub recover_ns: u64,
    pub serialization_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NoDummyFetchTrace {
    pub selected_physical_indices: Vec<usize>,
    pub outputs: Vec<ReconstructedRecord>,
    pub query_ciphertext_count: usize,
    pub response_ciphertext_count: usize,
    pub query_bytes: u64,
    pub response_bytes: u64,
    pub selected_payloads_exact: bool,
    pub unselected_windows_zero: bool,
    pub empty_response_all_zero: bool,
    pub timings: NoDummyFetchTimings,
}

pub struct NoDummyFetch {
    kernel: BgvCpir,
    database: PreparedDatabase,
    ids: Vec<RecordId>,
    frames: Vec<[u8; crate::FRAME_LEN]>,
    preparation: NoDummyFetchPreparation,
}

impl NoDummyFetch {
    pub fn prepare(records: &[LogicalRecord], profile: BgvProfile) -> Result<Self, P0dError> {
        let (kernel, setup) = BgvCpir::new_with_setup_timings(profile.config())
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let frames = records
            .iter()
            .map(encode_frame)
            .collect::<Result<Vec<_>, _>>()?;
        let packed_records = frames
            .iter()
            .map(|frame| frame.to_vec())
            .collect::<Vec<_>>();
        let database_start = Instant::now();
        let database = kernel
            .prepare_database(&packed_records)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let database_preparation_ns = elapsed_ns(database_start);
        let preparation = NoDummyFetchPreparation {
            profile: profile.label(),
            requested_ring_dimension: profile.ring_dimension(),
            actual_ring_dimension: kernel.actual_ring_dimension(),
            record_slots: kernel.record_slots(),
            real_record_count: records.len(),
            physical_capacity: kernel.capacity(),
            physical_dummy_window_count: 0,
            context_generation_ns: setup.context_generation_ns,
            key_generation_ns: setup.key_generation_ns,
            database_preparation_ns,
        };
        Ok(Self {
            kernel,
            database,
            ids: records.iter().map(|record| record.id).collect(),
            frames,
            preparation,
        })
    }

    #[must_use]
    pub const fn preparation(&self) -> &NoDummyFetchPreparation {
        &self.preparation
    }

    pub fn run(
        &mut self,
        selected_physical_indices: &[usize],
        artifact_directory: &Path,
    ) -> Result<NoDummyFetchTrace, P0dError> {
        if let Some(index) = selected_physical_indices
            .iter()
            .find(|&&index| index >= self.frames.len())
        {
            return Err(P0dError::FetchComponent(format!(
                "selected physical index {index} outside real-record count {}",
                self.frames.len()
            )));
        }
        let query_start = Instant::now();
        let query = self
            .kernel
            .encrypt_window_query(selected_physical_indices)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let fetch_query_ns = elapsed_ns(query_start);

        let response_start = Instant::now();
        let response = self.kernel.answer(&mut self.database, &query);
        let fetch_response_ns = elapsed_ns(response_start);

        let recover_start = Instant::now();
        let slots = self
            .kernel
            .decrypt_slots(&response)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let selected = selected_physical_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut selected_payloads_exact = true;
        let mut unselected_windows_zero = true;
        for index in 0..self.kernel.capacity() {
            let start = index * self.kernel.record_slots();
            let end = start + self.kernel.record_slots();
            let window = &slots[start..end];
            if selected.contains(&index) {
                let expected = self.frames[index]
                    .iter()
                    .copied()
                    .map(u64::from)
                    .collect::<Vec<_>>();
                selected_payloads_exact &= window == expected;
            } else {
                unselected_windows_zero &= window.iter().all(|&value| value == 0);
            }
        }
        let outputs = selected_physical_indices
            .iter()
            .map(|&index| {
                let frame = self
                    .kernel
                    .decode_fixed_window(&slots, index, crate::FRAME_LEN)
                    .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
                decode_frame(&frame, self.ids[index])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let empty_response_all_zero = slots.iter().all(|&value| value == 0);
        let recover_ns = elapsed_ns(recover_start);

        let serialization_start = Instant::now();
        let bundle = self
            .kernel
            .serialize_binary_bundle(artifact_directory, &query, &response)
            .map_err(|error| P0dError::Serialization(error.to_string()))?;
        let serialization_ns = elapsed_ns(serialization_start);

        Ok(NoDummyFetchTrace {
            selected_physical_indices: selected_physical_indices.to_vec(),
            outputs,
            query_ciphertext_count: 1,
            response_ciphertext_count: 1,
            query_bytes: bundle.sizes.query_bytes,
            response_bytes: bundle.sizes.response_bytes,
            selected_payloads_exact,
            unselected_windows_zero,
            empty_response_all_zero,
            timings: NoDummyFetchTimings {
                fetch_query_ns,
                fetch_response_ns,
                recover_ns,
                serialization_ns,
            },
        })
    }
}
