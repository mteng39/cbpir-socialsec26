use crate::{
    Catalog, FRAME_LEN, FetchTarget, P0dError, ReconstructedRecord, decode_frame, encode_frame,
};
use cbpir_bgv::{BgvCpir, BgvCpirConfig, PreparedDatabase};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Instant;

pub(crate) fn elapsed(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
fn error(e: impl std::fmt::Display) -> P0dError {
    P0dError::FetchComponent(e.to_string())
}
#[derive(Clone, Debug, Serialize)]
#[allow(
    clippy::struct_field_names,
    reason = "serialized measurements name their units explicitly"
)]
pub struct FetchTimings {
    pub query_ns: u64,
    pub server_ns: u64,
    pub decrypt_decode_ns: u64,
}
#[derive(Clone, Debug, Serialize)]
#[allow(
    clippy::struct_field_names,
    reason = "serialized measurements name their units explicitly"
)]
pub struct FetchPreparation {
    pub frame_encode_ns: u64,
    pub context_ns: u64,
    pub key_ns: u64,
    pub database_pack_ns: u64,
}
pub(crate) struct Fetch {
    kernel: BgvCpir,
    database: PreparedDatabase,
}
impl Fetch {
    pub(crate) fn prepare(c: &Catalog) -> Result<(Self, FetchPreparation), P0dError> {
        let version = BgvCpir::openfhe_version();
        if version != "1.5.1" {
            return Err(P0dError::Runtime(version));
        }
        let start = Instant::now();
        let mut frames = c
            .records()
            .iter()
            .map(|r| encode_frame(r).map(|f| f.to_vec()))
            .collect::<Result<Vec<_>, _>>()?;
        frames.push(vec![0; FRAME_LEN]);
        let frame_encode_ns = elapsed(start);
        let (kernel, t) =
            BgvCpir::new_with_setup_timings(BgvCpirConfig::prior_mini()).map_err(error)?;
        if kernel.actual_ring_dimension() != 8192
            || kernel.record_slots() != 128
            || kernel.capacity() != 64
        {
            return Err(error("unexpected mini layout"));
        }
        let start = Instant::now();
        let database = kernel.prepare_database(&frames).map_err(error)?;
        let database_pack_ns = elapsed(start);
        Ok((
            Self { kernel, database },
            FetchPreparation {
                frame_encode_ns,
                context_ns: t.context_generation_ns,
                key_ns: t.key_generation_ns,
                database_pack_ns,
            },
        ))
    }
    pub(crate) fn run(
        &mut self,
        c: &Catalog,
        target: FetchTarget,
    ) -> Result<(Option<ReconstructedRecord>, FetchTimings), P0dError> {
        c.validate_target(target)?;
        let start = Instant::now();
        let query = self.kernel.encrypt_query(target.index()).map_err(error)?;
        let query_ns = elapsed(start);
        let start = Instant::now();
        let response = self.kernel.answer(&mut self.database, &query);
        let server_ns = elapsed(start);
        let start = Instant::now();
        let slots = self.kernel.decrypt_slots(&response).map_err(error)?;
        let frame = self
            .kernel
            .decode_fixed_window(&slots, target.index(), FRAME_LEN)
            .map_err(error)?;
        let output = match target {
            FetchTarget::Dummy { .. } => {
                if frame.iter().any(|b| *b != 0) {
                    return Err(P0dError::Frame("nonzero dummy response"));
                }
                None
            }
            FetchTarget::Real { id, physical_index } => {
                let record = decode_frame(&frame, id)?;
                if record.payload != c.records()[physical_index].payload {
                    return Err(P0dError::Frame("reconstructed payload differs from source"));
                }
                Some(record)
            }
        };
        Ok((
            output,
            FetchTimings {
                query_ns,
                server_ns,
                decrypt_decode_ns: elapsed(start),
            },
        ))
    }
    pub(crate) fn sizes(&mut self, c: &Catalog) -> Result<Value, P0dError> {
        let query = self.kernel.encrypt_query(0).map_err(error)?;
        let response = self.kernel.answer(&mut self.database, &query);
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
        std::fs::create_dir_all(&base).map_err(error)?;
        let temporary = tempfile::Builder::new()
            .prefix("p0d-bgv-probe-")
            .tempdir_in(base)
            .map_err(error)?;
        let bundle = self
            .kernel
            .serialize_binary_bundle(temporary.path(), &query, &response)
            .map_err(|e| P0dError::Serialization(e.to_string()))?;
        let slots = BgvCpir::decrypt_serialized_response(&bundle.paths)
            .map_err(|e| P0dError::Serialization(e.to_string()))?;
        let decoded = self
            .kernel
            .decode_fixed_window(&slots, 0, FRAME_LEN)
            .map_err(error)?;
        if decoded != encode_frame(&c.records()[0])? {
            return Err(P0dError::Frame("serialization roundtrip mismatch"));
        }
        let s = bundle.sizes;
        Ok(
            json!({"scheme":"BGV-RNS","openfhe_version":BgvCpir::openfhe_version(),"context_bytes":s.context_bytes,"public_key_bytes":s.public_key_bytes,
            "private_key_bytes":s.private_key_bytes,"query_bytes":s.query_bytes,"response_bytes":s.response_bytes,"evaluation_key_bytes":s.evaluation_key_bytes,
            "roundtrip_exact":true,"artifact_files_retained":false,"execution":"separate randomized probe"}),
        )
    }
}
