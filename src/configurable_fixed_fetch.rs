//! Final configuration-study Fetch adapter over one uniform fixed width.

use crate::{
    FRAME_LEN, LogicalRecord, P0dError, ReconstructedRecord, RecordId, decode_frame, encode_frame,
};
use cbpir_bgv::{BgvCpir, BgvProfile, PreparedDatabase};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

pub const ALLOWED_FIXED_WINDOW_SLOTS: [usize; 6] = [64, 128, 224, 256, 384, 512];
const FRAME_HEADER_LEN: usize = 14;

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FixedFrameInspection {
    pub payload_length: usize,
    pub required_slots: usize,
    pub legacy_padding_slots: usize,
    pub legacy_padding_zero: bool,
    pub round_trip_exact: bool,
}

pub fn inspect_fixed_frame(record: &LogicalRecord) -> Result<FixedFrameInspection, P0dError> {
    let frame = encode_frame(record)?;
    if &frame[..4] != b"P0D1" {
        return Err(P0dError::Frame("wrong magic after encode"));
    }
    let mut id_bytes = [0_u8; 8];
    id_bytes.copy_from_slice(&frame[4..12]);
    if u64::from_le_bytes(id_bytes) != record.id.get() {
        return Err(P0dError::Frame("wrong ID after encode"));
    }
    let payload_length = usize::from(u16::from_le_bytes([frame[12], frame[13]]));
    let required_slots = FRAME_HEADER_LEN
        .checked_add(payload_length)
        .ok_or(P0dError::Frame("required slot count overflow"))?;
    if required_slots > FRAME_LEN {
        return Err(P0dError::Frame("required slots exceed legacy frame"));
    }
    let legacy_padding_zero = frame[required_slots..].iter().all(|&value| value == 0);
    if !legacy_padding_zero {
        return Err(P0dError::Frame("nonzero legacy frame padding"));
    }
    let recovered = decode_frame(&frame, record.id)?;
    let round_trip_exact = recovered.id == record.id && recovered.payload == record.payload;
    if !round_trip_exact {
        return Err(P0dError::Frame("legacy frame round trip changed record"));
    }
    Ok(FixedFrameInspection {
        payload_length,
        required_slots,
        legacy_padding_slots: FRAME_LEN - required_slots,
        legacy_padding_zero,
        round_trip_exact,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FixedWindowLayoutEntry {
    pub id: RecordId,
    pub physical_index: usize,
    pub payload_bytes: usize,
    pub required_slots: usize,
    pub record_slots: usize,
    pub window_start: usize,
    pub window_end: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigurableFixedFetchPreparation {
    pub profile: &'static str,
    pub requested_ring_dimension: usize,
    pub actual_ring_dimension: usize,
    pub record_slots: usize,
    pub real_record_count: usize,
    pub physical_capacity: usize,
    pub physical_dummy_window_count: usize,
    pub max_required_slots: usize,
    pub unused_ring_tail_slots: usize,
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
    pub database_preparation_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigurableFixedFetchTimings {
    pub fetch_query_ns: u64,
    pub fetch_response_ns: u64,
    pub recover_ns: u64,
    pub serialization_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigurableFixedFetchTrace {
    pub selected_physical_indices: Vec<usize>,
    pub outputs: Vec<ReconstructedRecord>,
    pub query_ciphertext_count: usize,
    pub response_ciphertext_count: usize,
    pub query_bytes: u64,
    pub response_bytes: u64,
    pub selected_payloads_exact: bool,
    pub unselected_windows_zero: bool,
    pub unused_tail_zero: bool,
    pub empty_response_all_zero: bool,
    pub timings: ConfigurableFixedFetchTimings,
}

pub struct ConfigurableFixedFetch {
    kernel: BgvCpir,
    database: PreparedDatabase,
    ids: Vec<RecordId>,
    frames: Vec<[u8; FRAME_LEN]>,
    packed_frames: Vec<Vec<u8>>,
    layout: Vec<FixedWindowLayoutEntry>,
    preparation: ConfigurableFixedFetchPreparation,
}

impl ConfigurableFixedFetch {
    pub fn prepare(
        records: &[LogicalRecord],
        profile: BgvProfile,
        record_slots: usize,
    ) -> Result<Self, P0dError> {
        if !ALLOWED_FIXED_WINDOW_SLOTS.contains(&record_slots) {
            return Err(P0dError::FetchComponent(format!(
                "record width {record_slots} is outside the allowed fixed-window set"
            )));
        }

        let frames = records
            .iter()
            .map(encode_frame)
            .collect::<Result<Vec<_>, _>>()?;
        let inspections = records
            .iter()
            .map(inspect_fixed_frame)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some((index, inspection)) = inspections
            .iter()
            .enumerate()
            .find(|(_, inspection)| inspection.required_slots > record_slots)
        {
            return Err(P0dError::FetchComponent(format!(
                "record {index} requires {} slots but fixed width is {record_slots}",
                inspection.required_slots
            )));
        }

        let packed_frames = frames
            .iter()
            .map(|frame| {
                let mut packed = vec![0_u8; record_slots];
                let copied = record_slots.min(FRAME_LEN);
                packed[..copied].copy_from_slice(&frame[..copied]);
                packed
            })
            .collect::<Vec<_>>();
        let layout = records
            .iter()
            .zip(&inspections)
            .enumerate()
            .map(
                |(physical_index, (record, inspection))| FixedWindowLayoutEntry {
                    id: record.id,
                    physical_index,
                    payload_bytes: record.payload.len(),
                    required_slots: inspection.required_slots,
                    record_slots,
                    window_start: physical_index * record_slots,
                    window_end: (physical_index + 1) * record_slots,
                },
            )
            .collect::<Vec<_>>();

        let mut config = profile.config();
        config.record_slots = record_slots;
        let (kernel, setup) = BgvCpir::new_with_setup_timings(config)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let database_start = Instant::now();
        let database = kernel
            .prepare_database(&packed_frames)
            .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
        let database_preparation_ns = elapsed_ns(database_start);
        let physical_capacity = kernel.capacity();
        let preparation = ConfigurableFixedFetchPreparation {
            profile: profile.label(),
            requested_ring_dimension: profile.ring_dimension(),
            actual_ring_dimension: kernel.actual_ring_dimension(),
            record_slots,
            real_record_count: records.len(),
            physical_capacity,
            physical_dummy_window_count: 0,
            max_required_slots: inspections
                .iter()
                .map(|inspection| inspection.required_slots)
                .max()
                .unwrap_or(0),
            unused_ring_tail_slots: kernel.actual_ring_dimension()
                - physical_capacity * record_slots,
            context_generation_ns: setup.context_generation_ns,
            key_generation_ns: setup.key_generation_ns,
            database_preparation_ns,
        };
        Ok(Self {
            kernel,
            database,
            ids: records.iter().map(|record| record.id).collect(),
            frames,
            packed_frames,
            layout,
            preparation,
        })
    }

    #[must_use]
    pub const fn preparation(&self) -> &ConfigurableFixedFetchPreparation {
        &self.preparation
    }

    #[must_use]
    pub fn layout(&self) -> &[FixedWindowLayoutEntry] {
        &self.layout
    }

    pub fn run(
        &mut self,
        selected_physical_indices: &[usize],
        artifact_directory: &Path,
    ) -> Result<ConfigurableFixedFetchTrace, P0dError> {
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
        let width = self.preparation.record_slots;
        let mut selected_payloads_exact = true;
        let mut unselected_windows_zero = true;
        for index in 0..self.preparation.physical_capacity {
            let start = index * width;
            let end = start + width;
            let window = &slots[start..end];
            if selected.contains(&index) {
                selected_payloads_exact &= window
                    == self.packed_frames[index]
                        .iter()
                        .copied()
                        .map(u64::from)
                        .collect::<Vec<_>>();
            } else {
                unselected_windows_zero &= window.iter().all(|&value| value == 0);
            }
        }
        let used_slots = self.preparation.physical_capacity * width;
        let unused_tail_zero = slots[used_slots..].iter().all(|&value| value == 0);

        let outputs = selected_physical_indices
            .iter()
            .map(|&index| {
                let recovered = self
                    .kernel
                    .decode_fixed_window(&slots, index, width)
                    .map_err(|error| P0dError::FetchComponent(error.to_string()))?;
                if recovered[FRAME_LEN.min(width)..]
                    .iter()
                    .any(|&value| value != 0)
                {
                    return Err(P0dError::Frame("nonzero slots beyond legacy frame"));
                }
                let mut frame = [0_u8; FRAME_LEN];
                let copied = FRAME_LEN.min(width);
                frame[..copied].copy_from_slice(&recovered[..copied]);
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

        Ok(ConfigurableFixedFetchTrace {
            selected_physical_indices: selected_physical_indices.to_vec(),
            outputs,
            query_ciphertext_count: 1,
            response_ciphertext_count: 1,
            query_bytes: bundle.sizes.query_bytes,
            response_bytes: bundle.sizes.response_bytes,
            selected_payloads_exact,
            unselected_windows_zero,
            unused_tail_zero,
            empty_response_all_zero,
            timings: ConfigurableFixedFetchTimings {
                fetch_query_ns,
                fetch_response_ns,
                recover_ns,
                serialization_ns,
            },
        })
    }
}
