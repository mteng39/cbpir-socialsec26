//! Streamed `OpenFHE` BGV execution for sharded, chunked P3 payloads.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Instant;

use cbpir_bgv::{ArtifactPaths, ArtifactSizes, BgvCpir, BgvCpirConfig, EncryptedQuery};

use crate::{
    BASE_WINDOW_BYTES, FetchedRecord, PHYSICAL_WINDOWS_PER_SHARD, RecordId, ScaledFetchPlan,
    ScaledLayout, ScaledP3Error,
};

/// One-time context and key-generation costs for a scaled backend instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScaledSetupTimings {
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
}

/// Accumulated phases for one logical query-all-shards Fetch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScaledFetchTimings {
    pub database_pack_encode_ns: u64,
    pub query_encode_encrypt_ns: u64,
    pub server_multiply_ns: u64,
    pub client_decrypt_decode_ns: u64,
    pub reconstruction_ns: u64,
    pub total_ns: u64,
}

/// Physical cryptographic operation counts for one logical Fetch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScaledFetchCounts {
    pub query_ciphertexts: usize,
    pub response_ciphertexts: usize,
    pub server_multiplications: usize,
    pub evaluated_chunk_planes: usize,
}

/// Reconstructed records plus measured phase timings and physical counts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaledFetchResult {
    pub records: Vec<FetchedRecord>,
    pub timings: ScaledFetchTimings,
    pub counts: ScaledFetchCounts,
}

/// Memory-bounded sharded P3 backend over the unchanged P0-A BGV kernel.
pub struct ShardedP3BgvBackend {
    kernel: BgvCpir,
    records: Vec<Vec<u8>>,
    layout: ScaledLayout,
}

struct FetchState {
    outputs: Vec<Vec<u8>>,
    output_positions: HashMap<RecordId, usize>,
    received_chunks: Vec<usize>,
    timings: ScaledFetchTimings,
    counts: ScaledFetchCounts,
}

impl ShardedP3BgvBackend {
    /// Validates exact payload widths and creates one reusable HE context/key pair.
    ///
    /// The raw database remains in memory. Packed plaintexts are produced one
    /// shard/chunk plane at a time during each Fetch and are not cached.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported workload/configuration, inconsistent
    /// record width, P0-A context failure, or unexpected native ring layout.
    pub fn new(
        records: Vec<Vec<u8>>,
        public_k: usize,
        config: BgvCpirConfig,
    ) -> Result<(Self, ScaledSetupTimings), ScaledP3Error> {
        let payload_bytes = records.first().map_or(0, Vec::len);
        let layout = ScaledLayout::new(records.len(), payload_bytes, public_k)?;
        if config != BgvCpirConfig::prior_mini() {
            return Err(ScaledP3Error::UnsupportedKernelConfig);
        }
        for (index, record) in records.iter().enumerate() {
            if record.len() != payload_bytes {
                return Err(ScaledP3Error::RecordWidth {
                    index,
                    actual: record.len(),
                    expected: payload_bytes,
                });
            }
        }

        let (kernel, setup) = BgvCpir::new_with_setup_timings(config)?;
        if kernel.actual_ring_dimension() != 8192
            || kernel.record_slots() != BASE_WINDOW_BYTES
            || kernel.capacity() != PHYSICAL_WINDOWS_PER_SHARD
        {
            return Err(ScaledP3Error::UnexpectedKernelLayout {
                ring_dimension: kernel.actual_ring_dimension(),
                record_slots: kernel.record_slots(),
                capacity: kernel.capacity(),
            });
        }

        Ok((
            Self {
                kernel,
                records,
                layout,
            },
            ScaledSetupTimings {
                context_generation_ns: setup.context_generation_ns,
                key_generation_ns: setup.key_generation_ns,
            },
        ))
    }

    /// Returns the public scaled layout.
    #[must_use]
    pub const fn layout(&self) -> ScaledLayout {
        self.layout
    }

    /// Builds the client-held global and per-shard fixed-`K` plans.
    ///
    /// # Errors
    ///
    /// Returns an error for any invalid candidate or checked planning failure.
    pub fn plan(&self, candidates: &[RecordId]) -> Result<ScaledFetchPlan, ScaledP3Error> {
        self.layout.plan(candidates)
    }

    /// Executes all public shards and payload chunk planes, then reconstructs
    /// selected records in stable global candidate order.
    ///
    /// # Errors
    ///
    /// Returns an error for a foreign plan, HE/packing/decode failure, a slot
    /// mismatch, an operation-count mismatch, or incomplete reconstruction.
    pub fn fetch(&mut self, plan: &ScaledFetchPlan) -> Result<ScaledFetchResult, ScaledP3Error> {
        self.validate_plan_layout(plan)?;

        let total_started = Instant::now();
        let mut state = FetchState::new(plan, self.layout.payload_bytes());
        for shard in 0..self.layout.shard_count() {
            self.execute_shard(plan, shard, &mut state)?;
        }
        self.validate_counts(&state.counts)?;
        state.finish(plan, self.layout.chunk_count(), total_started)
    }

    /// Serializes one representative shard/chunk exchange, decrypts it through
    /// fresh wrapper objects, and removes every generated artifact.
    ///
    /// Returned lengths characterize one physical query and one physical
    /// response. They are not a serialization of the complete logical Fetch.
    ///
    /// # Errors
    ///
    /// Returns an error for a foreign plan, HE/serialization failure,
    /// round-trip mismatch, or artifact-cleanup failure. Cleanup is attempted
    /// for all five paths before any exchange error is returned.
    pub fn serialize_representative(
        &self,
        plan: &ScaledFetchPlan,
        directory: &Path,
    ) -> Result<ArtifactSizes, ScaledP3Error> {
        self.validate_plan_layout(plan)?;
        let shard = 0;
        let chunk = 0;
        let indices = plan
            .shard_physical_ids(shard)?
            .iter()
            .map(|id| {
                usize::try_from(id.0)
                    .map_err(|_| ScaledP3Error::ArithmeticOverflow("local physical ID"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let query = self.kernel.encrypt_window_query(&indices)?;
        let physical_records = self.assemble_chunk_plane(shard, chunk)?;
        let mut database = self.kernel.prepare_database(&physical_records)?;
        let response = self.kernel.answer(&mut database, &query);
        let original_slots = self.kernel.decrypt_slots(&response)?;
        self.validate_decrypted_slots(shard, chunk, &physical_records, &indices, &original_slots)?;

        let paths = artifact_paths(directory);
        let exchange_result = (|| {
            let bundle = self
                .kernel
                .serialize_binary_bundle(directory, &query, &response)?;
            let round_trip_slots = BgvCpir::decrypt_serialized_response(&bundle.paths)?;
            compare_slots(shard, chunk, &original_slots, &round_trip_slots)?;
            Ok(bundle.sizes)
        })();
        let cleanup_result = cleanup_artifacts(&paths);
        if let Err(error) = cleanup_result {
            return Err(error.into());
        }
        exchange_result
    }

    fn execute_shard(
        &mut self,
        plan: &ScaledFetchPlan,
        shard: usize,
        state: &mut FetchState,
    ) -> Result<(), ScaledP3Error> {
        let indices = plan
            .shard_physical_ids(shard)?
            .iter()
            .map(|id| {
                usize::try_from(id.0)
                    .map_err(|_| ScaledP3Error::ArithmeticOverflow("local physical ID"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let query_started = Instant::now();
        let query = self.kernel.encrypt_window_query(&indices)?;
        state.timings.query_encode_encrypt_ns = state
            .timings
            .query_encode_encrypt_ns
            .saturating_add(elapsed_ns(query_started));
        checked_increment(&mut state.counts.query_ciphertexts, "query ciphertexts")?;

        for chunk in 0..self.layout.chunk_count() {
            self.execute_chunk_plane(plan, shard, chunk, &indices, &query, state)?;
        }
        Ok(())
    }

    fn execute_chunk_plane(
        &mut self,
        plan: &ScaledFetchPlan,
        shard: usize,
        chunk: usize,
        indices: &[usize],
        query: &EncryptedQuery,
        state: &mut FetchState,
    ) -> Result<(), ScaledP3Error> {
        let database_started = Instant::now();
        let physical_records = self.assemble_chunk_plane(shard, chunk)?;
        let mut database = self.kernel.prepare_database(&physical_records)?;
        state.timings.database_pack_encode_ns = state
            .timings
            .database_pack_encode_ns
            .saturating_add(elapsed_ns(database_started));

        let server_started = Instant::now();
        let response = self.kernel.answer(&mut database, query);
        state.timings.server_multiply_ns = state
            .timings
            .server_multiply_ns
            .saturating_add(elapsed_ns(server_started));
        checked_increment(
            &mut state.counts.server_multiplications,
            "server multiplications",
        )?;
        checked_increment(
            &mut state.counts.response_ciphertexts,
            "response ciphertexts",
        )?;
        checked_increment(
            &mut state.counts.evaluated_chunk_planes,
            "evaluated chunk planes",
        )?;

        let client_started = Instant::now();
        let slots = self.kernel.decrypt_slots(&response)?;
        self.validate_decrypted_slots(shard, chunk, &physical_records, indices, &slots)?;
        let selected_chunks = indices
            .iter()
            .take(plan.shard_selected_real_count(shard)?)
            .map(|&index| {
                self.kernel
                    .decode_fixed_window(&slots, index, BASE_WINDOW_BYTES)
            })
            .collect::<Result<Vec<_>, _>>()?;
        state.timings.client_decrypt_decode_ns = state
            .timings
            .client_decrypt_decode_ns
            .saturating_add(elapsed_ns(client_started));
        state.reconstruct_chunk(
            plan.shard_selected_global_ids(shard)?,
            selected_chunks,
            chunk,
            self.layout.chunk_count(),
        )
    }

    fn validate_counts(&self, counts: &ScaledFetchCounts) -> Result<(), ScaledP3Error> {
        check_count(
            "query ciphertexts",
            self.layout.query_ciphertext_count()?,
            counts.query_ciphertexts,
        )?;
        check_count(
            "response ciphertexts",
            self.layout.response_ciphertext_count()?,
            counts.response_ciphertexts,
        )?;
        check_count(
            "server multiplications",
            self.layout.server_multiplication_count()?,
            counts.server_multiplications,
        )?;
        check_count(
            "evaluated chunk planes",
            self.layout.response_ciphertext_count()?,
            counts.evaluated_chunk_planes,
        )
    }

    fn validate_plan_layout(&self, plan: &ScaledFetchPlan) -> Result<(), ScaledP3Error> {
        if plan.layout() == self.layout {
            Ok(())
        } else {
            Err(ScaledP3Error::PlanLayoutMismatch {
                expected: self.layout,
                actual: plan.layout(),
            })
        }
    }

    fn assemble_chunk_plane(
        &self,
        shard: usize,
        chunk: usize,
    ) -> Result<Vec<Vec<u8>>, ScaledP3Error> {
        if chunk >= self.layout.chunk_count() {
            return Err(ScaledP3Error::ChunkOutOfRange {
                chunk,
                chunk_count: self.layout.chunk_count(),
            });
        }
        let real_count = self.layout.shard_real_record_count(shard)?;
        let shard_start = shard
            .checked_mul(self.layout.real_rows_per_shard())
            .ok_or(ScaledP3Error::ArithmeticOverflow("shard start"))?;
        let chunk_start = chunk
            .checked_mul(BASE_WINDOW_BYTES)
            .ok_or(ScaledP3Error::ArithmeticOverflow("chunk start"))?;
        let chunk_end = chunk_start
            .checked_add(BASE_WINDOW_BYTES)
            .ok_or(ScaledP3Error::ArithmeticOverflow("chunk end"))?;
        let physical_count = real_count
            .checked_add(self.layout.public_k())
            .ok_or(ScaledP3Error::ArithmeticOverflow("physical rows"))?;
        let mut physical_records = Vec::with_capacity(physical_count);

        for local_index in 0..real_count {
            let global_index = shard_start
                .checked_add(local_index)
                .ok_or(ScaledP3Error::ArithmeticOverflow("global record index"))?;
            physical_records.push(self.records[global_index][chunk_start..chunk_end].to_vec());
        }
        physical_records.extend((0..self.layout.public_k()).map(|_| vec![0_u8; BASE_WINDOW_BYTES]));
        Ok(physical_records)
    }

    fn validate_decrypted_slots(
        &self,
        shard: usize,
        chunk: usize,
        physical_records: &[Vec<u8>],
        selected_indices: &[usize],
        actual_slots: &[u64],
    ) -> Result<(), ScaledP3Error> {
        let mut expected_slots = vec![0_u64; self.kernel.actual_ring_dimension()];
        for &index in selected_indices {
            let record = &physical_records[index];
            let start = index
                .checked_mul(BASE_WINDOW_BYTES)
                .ok_or(ScaledP3Error::ArithmeticOverflow("slot start"))?;
            for (offset, &byte) in record.iter().enumerate() {
                expected_slots[start + offset] = u64::from(byte);
            }
        }
        for (position, (&expected, &actual)) in expected_slots.iter().zip(actual_slots).enumerate()
        {
            if expected != actual {
                return Err(ScaledP3Error::SlotMismatch {
                    shard,
                    chunk,
                    position,
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }
}

impl FetchState {
    fn new(plan: &ScaledFetchPlan, payload_bytes: usize) -> Self {
        let started = Instant::now();
        let outputs = plan
            .selected_ids()
            .iter()
            .map(|_| vec![0_u8; payload_bytes])
            .collect::<Vec<_>>();
        let output_positions = plan
            .selected_ids()
            .iter()
            .copied()
            .enumerate()
            .map(|(position, id)| (id, position))
            .collect();
        let received_chunks = vec![0_usize; outputs.len()];
        Self {
            outputs,
            output_positions,
            received_chunks,
            timings: ScaledFetchTimings {
                reconstruction_ns: elapsed_ns(started),
                ..ScaledFetchTimings::default()
            },
            counts: ScaledFetchCounts::default(),
        }
    }

    fn reconstruct_chunk(
        &mut self,
        global_ids: &[RecordId],
        chunks: Vec<Vec<u8>>,
        chunk: usize,
        chunk_count: usize,
    ) -> Result<(), ScaledP3Error> {
        check_count("selected payload chunks", global_ids.len(), chunks.len())?;
        let started = Instant::now();
        let chunk_start = chunk
            .checked_mul(BASE_WINDOW_BYTES)
            .ok_or(ScaledP3Error::ArithmeticOverflow("chunk start"))?;
        let chunk_end = chunk_start
            .checked_add(BASE_WINDOW_BYTES)
            .ok_or(ScaledP3Error::ArithmeticOverflow("chunk end"))?;
        for (&global_id, chunk_bytes) in global_ids.iter().zip(chunks) {
            let output_position =
                *self
                    .output_positions
                    .get(&global_id)
                    .ok_or(ScaledP3Error::MissingChunk {
                        id: global_id.0,
                        expected: chunk_count,
                        actual: 0,
                    })?;
            self.outputs[output_position][chunk_start..chunk_end].copy_from_slice(&chunk_bytes);
            checked_increment(
                &mut self.received_chunks[output_position],
                "received payload chunks",
            )?;
        }
        self.timings.reconstruction_ns = self
            .timings
            .reconstruction_ns
            .saturating_add(elapsed_ns(started));
        Ok(())
    }

    fn finish(
        mut self,
        plan: &ScaledFetchPlan,
        chunk_count: usize,
        total_started: Instant,
    ) -> Result<ScaledFetchResult, ScaledP3Error> {
        for (&id, &actual) in plan.selected_ids().iter().zip(&self.received_chunks) {
            if actual != chunk_count {
                return Err(ScaledP3Error::MissingChunk {
                    id: id.0,
                    expected: chunk_count,
                    actual,
                });
            }
        }

        let reconstruction_started = Instant::now();
        let records = plan
            .selected_ids()
            .iter()
            .copied()
            .zip(self.outputs)
            .map(|(id, bytes)| FetchedRecord::from_parts(id, bytes))
            .collect();
        self.timings.reconstruction_ns = self
            .timings
            .reconstruction_ns
            .saturating_add(elapsed_ns(reconstruction_started));
        self.timings.total_ns = elapsed_ns(total_started);
        Ok(ScaledFetchResult {
            records,
            timings: self.timings,
            counts: self.counts,
        })
    }
}

fn checked_increment(value: &mut usize, label: &'static str) -> Result<(), ScaledP3Error> {
    *value = value
        .checked_add(1)
        .ok_or(ScaledP3Error::ArithmeticOverflow(label))?;
    Ok(())
}

fn check_count(label: &'static str, expected: usize, actual: usize) -> Result<(), ScaledP3Error> {
    if expected == actual {
        Ok(())
    } else {
        Err(ScaledP3Error::ObservedCountMismatch {
            label,
            expected,
            actual,
        })
    }
}

fn artifact_paths(directory: &Path) -> ArtifactPaths {
    ArtifactPaths {
        context: directory.join("context.bin"),
        public_key: directory.join("public_key.bin"),
        private_key: directory.join("private_key.bin"),
        query: directory.join("query.bin"),
        response: directory.join("response.bin"),
    }
}

fn cleanup_artifacts(paths: &ArtifactPaths) -> Result<(), std::io::Error> {
    let mut first_error = None;
    for path in [
        &paths.context,
        &paths.public_key,
        &paths.private_key,
        &paths.query,
        &paths.response,
    ] {
        if let Err(error) = fs::remove_file(path)
            && error.kind() != ErrorKind::NotFound
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn compare_slots(
    shard: usize,
    chunk: usize,
    expected_slots: &[u64],
    actual_slots: &[u64],
) -> Result<(), ScaledP3Error> {
    check_count("round-trip slots", expected_slots.len(), actual_slots.len())?;
    for (position, (&expected, &actual)) in expected_slots.iter().zip(actual_slots).enumerate() {
        if expected != actual {
            return Err(ScaledP3Error::SlotMismatch {
                shard,
                chunk,
                position,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
