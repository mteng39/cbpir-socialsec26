use crate::{
    Candidate, Catalog, FRAME_LEN, P0dError, ReconstructedRecord, decode_frame, encode_frame,
};
use cbpir_bgv::{BgvCpir, BgvCpirConfig};
use cbpir_fetch::{P3BgvBackend, RecordId as P3RecordId};
use serde::Serialize;
use std::collections::HashSet;
use std::time::Instant;

use crate::fetch_stage::elapsed;

fn error(error: impl std::fmt::Display) -> P0dError {
    P0dError::FetchComponent(error.to_string())
}

#[derive(Clone, Debug, Serialize)]
pub struct MultiFetchPreparation {
    pub frame_encode_ns: u64,
    pub context_ns: u64,
    pub key_ns: u64,
    pub database_pack_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MultiFetchTimings {
    pub query_ns: u64,
    pub server_ns: u64,
    pub decrypt_decode_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MultiFetchTrace {
    pub selected_physical_indices: Vec<usize>,
    pub planned_physical_indices: Vec<usize>,
    pub dummy_flags: Vec<bool>,
    pub selected_real_count: usize,
    pub dummy_count: usize,
    pub unselected_windows_zero: bool,
}

pub(crate) struct MultiFetch {
    backend: P3BgvBackend,
    frames: Vec<Vec<u8>>,
}

impl MultiFetch {
    pub(crate) fn prepare(
        catalog: &Catalog,
        public_k: usize,
    ) -> Result<(Self, MultiFetchPreparation), P0dError> {
        let version = BgvCpir::openfhe_version();
        if version != "1.5.1" {
            return Err(P0dError::Runtime(version));
        }

        let started = Instant::now();
        let frames = catalog
            .records()
            .iter()
            .map(|record| encode_frame(record).map(|frame| frame.to_vec()))
            .collect::<Result<Vec<_>, _>>()?;
        let frame_encode_ns = elapsed(started);
        let (backend, setup) =
            P3BgvBackend::new(&frames, public_k, BgvCpirConfig::prior_mini()).map_err(error)?;
        if backend.kernel().actual_ring_dimension() != 8192
            || backend.kernel().record_slots() != FRAME_LEN
            || backend.kernel().capacity() != 64
        {
            return Err(error("unexpected P3 mini layout"));
        }

        Ok((
            Self { backend, frames },
            MultiFetchPreparation {
                frame_encode_ns,
                context_ns: setup.context_generation_ns,
                key_ns: setup.key_generation_ns,
                database_pack_ns: setup.database_assembly_and_encode_ns,
            },
        ))
    }

    pub(crate) fn run(
        &mut self,
        catalog: &Catalog,
        selected: &[Candidate],
    ) -> Result<(Vec<ReconstructedRecord>, MultiFetchTrace, MultiFetchTimings), P0dError> {
        let physical_ids = selected
            .iter()
            .map(|candidate| {
                u64::try_from(candidate.physical_index)
                    .map(P3RecordId)
                    .map_err(|_| error("physical index cannot be represented as u64"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let plan = self.backend.planner().plan(&physical_ids).map_err(error)?;
        if plan.selected_real_count() != selected.len() || plan.truncated_unique_count() != 0 {
            return Err(error("P3 changed the client-selected real set"));
        }

        let result = self.backend.fetch(&plan).map_err(error)?;
        if result.records.len() != selected.len() {
            return Err(error("P3 returned the wrong real-record count"));
        }

        let mut outputs = Vec::with_capacity(selected.len());
        for (candidate, fetched) in selected.iter().zip(&result.records) {
            let expected_physical = u64::try_from(candidate.physical_index)
                .map_err(|_| error("physical index cannot be represented as u64"))?;
            if fetched.id().0 != expected_physical {
                return Err(error("P3 returned a different physical row"));
            }
            let source = catalog
                .records()
                .get(candidate.physical_index)
                .ok_or(P0dError::Target("physical row outside catalog"))?;
            if source.id != candidate.id {
                return Err(P0dError::Target("stable ID and physical row disagree"));
            }
            let output = decode_frame(fetched.bytes(), candidate.id)?;
            if output.payload != source.payload {
                return Err(P0dError::Frame("reconstructed payload differs from source"));
            }
            outputs.push(output);
        }

        validate_slot_isolation(&result.decrypted_slots, &self.frames, &physical_ids)?;
        let planned_physical_indices = plan
            .physical_ids()
            .iter()
            .map(|id| usize::try_from(id.0).map_err(|_| error("P3 physical row exceeds usize")))
            .collect::<Result<Vec<_>, _>>()?;
        let selected_physical_indices = selected
            .iter()
            .map(|candidate| candidate.physical_index)
            .collect();
        let trace = MultiFetchTrace {
            selected_physical_indices,
            planned_physical_indices,
            dummy_flags: plan.is_dummy().to_vec(),
            selected_real_count: plan.selected_real_count(),
            dummy_count: plan.public_k() - plan.selected_real_count(),
            unselected_windows_zero: true,
        };
        let timings = MultiFetchTimings {
            query_ns: result.timings.query_encode_encrypt_ns,
            server_ns: result.timings.server_multiply_ns,
            decrypt_decode_ns: result.timings.client_decrypt_decode_ns,
        };
        Ok((outputs, trace, timings))
    }
}

fn validate_slot_isolation(
    slots: &[u64],
    frames: &[Vec<u8>],
    selected: &[P3RecordId],
) -> Result<(), P0dError> {
    if slots.len() != 8192 {
        return Err(error("P3 returned the wrong decrypted slot count"));
    }
    let selected = selected
        .iter()
        .map(|id| usize::try_from(id.0).map_err(|_| error("P3 physical row exceeds usize")))
        .collect::<Result<HashSet<_>, _>>()?;
    for physical_index in 0..64 {
        let start = physical_index * FRAME_LEN;
        let window = &slots[start..start + FRAME_LEN];
        if selected.contains(&physical_index) {
            let expected = frames
                .get(physical_index)
                .ok_or(P0dError::Target("selected physical row outside catalog"))?;
            if !window
                .iter()
                .copied()
                .eq(expected.iter().copied().map(u64::from))
            {
                return Err(P0dError::Frame("selected P3 window differs from source"));
            }
        } else if window.iter().any(|slot| *slot != 0) {
            return Err(P0dError::Frame("unselected P3 window is nonzero"));
        }
    }
    Ok(())
}
