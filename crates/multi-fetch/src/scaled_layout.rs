//! Checked layout arithmetic for proposal-sized sharded P3 retrieval.

use cbpir_bgv::{CpirError, SerializationError};
use thiserror::Error;

use crate::{FetchError, FetchPlan, FetchPlanner, RecordId, SUPPORTED_PUBLIC_K};

/// Byte width of one validated P0-A/P3 plaintext record window.
pub const BASE_WINDOW_BYTES: usize = 128;

/// Number of 128-byte windows carried by the validated 8192-slot ring.
pub const PHYSICAL_WINDOWS_PER_SHARD: usize = 64;

/// Proposal-defined database sizes covered by the scaling gate.
pub const SUPPORTED_DATABASE_RECORDS: [usize; 5] = [1 << 10, 1 << 11, 1 << 12, 1 << 13, 1 << 14];

/// Proposal-defined payload widths covered by the scaling gate.
pub const SUPPORTED_PAYLOAD_BYTES: [usize; 2] = [256, 1024];

/// Physical location of one real record in the sharded layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShardAddress {
    shard_index: usize,
    local_index: usize,
}

impl ShardAddress {
    /// Returns the zero-based shard index.
    #[must_use]
    pub const fn shard_index(self) -> usize {
        self.shard_index
    }

    /// Returns the zero-based real-row index within the shard.
    #[must_use]
    pub const fn local_index(self) -> usize {
        self.local_index
    }
}

/// Public shape of one proposal-sized, query-all-shards Fetch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScaledLayout {
    database_records: usize,
    payload_bytes: usize,
    public_k: usize,
    real_rows_per_shard: usize,
    shard_count: usize,
    chunk_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScaledShardPlan {
    shard_index: usize,
    global_ids: Vec<RecordId>,
    local_plan: FetchPlan,
}

/// Client-held global selection plus one fixed-`K` plan per public shard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaledFetchPlan {
    layout: ScaledLayout,
    selected_ids: Vec<RecordId>,
    truncated_unique_count: usize,
    shard_plans: Vec<ScaledShardPlan>,
}

impl ScaledLayout {
    /// Constructs a checked layout for one supported `(N, B, K)` tuple.
    ///
    /// # Errors
    ///
    /// Returns an error when any dimension is outside the declared scaling
    /// matrix or checked layout arithmetic fails.
    pub fn new(
        database_records: usize,
        payload_bytes: usize,
        public_k: usize,
    ) -> Result<Self, ScaledP3Error> {
        if !SUPPORTED_DATABASE_RECORDS.contains(&database_records) {
            return Err(ScaledP3Error::UnsupportedDatabaseRecords {
                actual: database_records,
            });
        }
        if !SUPPORTED_PAYLOAD_BYTES.contains(&payload_bytes) {
            return Err(ScaledP3Error::UnsupportedPayloadBytes {
                actual: payload_bytes,
            });
        }
        if !SUPPORTED_PUBLIC_K.contains(&public_k) {
            return Err(ScaledP3Error::UnsupportedPublicK { actual: public_k });
        }

        let real_rows_per_shard = PHYSICAL_WINDOWS_PER_SHARD
            .checked_sub(public_k)
            .ok_or(ScaledP3Error::ArithmeticOverflow("real rows per shard"))?;
        let shard_count = database_records.div_ceil(real_rows_per_shard);
        let chunk_count = payload_bytes / BASE_WINDOW_BYTES;

        Ok(Self {
            database_records,
            payload_bytes,
            public_k,
            real_rows_per_shard,
            shard_count,
            chunk_count,
        })
    }

    /// Returns public database cardinality `N`.
    #[must_use]
    pub const fn database_records(self) -> usize {
        self.database_records
    }

    /// Returns public payload width `B` in bytes.
    #[must_use]
    pub const fn payload_bytes(self) -> usize {
        self.payload_bytes
    }

    /// Returns public candidate bound `K`.
    #[must_use]
    pub const fn public_k(self) -> usize {
        self.public_k
    }

    /// Returns `64 - K` real rows allocated to every shard.
    #[must_use]
    pub const fn real_rows_per_shard(self) -> usize {
        self.real_rows_per_shard
    }

    /// Returns `ceil(N / (64 - K))` shards.
    #[must_use]
    pub const fn shard_count(self) -> usize {
        self.shard_count
    }

    /// Returns `B / 128` payload chunk planes.
    #[must_use]
    pub const fn chunk_count(self) -> usize {
        self.chunk_count
    }

    /// Returns the number of query ciphertexts in a fixed-shape Fetch.
    ///
    /// # Errors
    ///
    /// This accessor is fallible for a uniform checked-count interface; the
    /// currently supported layout cannot overflow this count.
    pub const fn query_ciphertext_count(self) -> Result<usize, ScaledP3Error> {
        Ok(self.shard_count)
    }

    /// Returns the number of response ciphertexts in a fixed-shape Fetch.
    ///
    /// # Errors
    ///
    /// Returns an error if multiplying shards by chunk planes overflows.
    pub fn response_ciphertext_count(self) -> Result<usize, ScaledP3Error> {
        self.shard_count
            .checked_mul(self.chunk_count)
            .ok_or(ScaledP3Error::ArithmeticOverflow(
                "response ciphertext count",
            ))
    }

    /// Returns the number of ciphertext-plaintext server multiplications.
    ///
    /// # Errors
    ///
    /// Returns an error if the equal response count overflows.
    pub fn server_multiplication_count(self) -> Result<usize, ScaledP3Error> {
        self.response_ciphertext_count()
    }

    /// Maps a real record index to its shard and local real-row index.
    ///
    /// # Errors
    ///
    /// Returns an error if `id` is outside the real database prefix.
    pub fn map_record(self, id: usize) -> Result<ShardAddress, ScaledP3Error> {
        if id >= self.database_records {
            return Err(ScaledP3Error::CandidateOutOfRange {
                id: u64::try_from(id).unwrap_or(u64::MAX),
                database_records: self.database_records,
            });
        }
        Ok(ShardAddress {
            shard_index: id / self.real_rows_per_shard,
            local_index: id % self.real_rows_per_shard,
        })
    }

    /// Returns the number of real records in one shard.
    ///
    /// # Errors
    ///
    /// Returns an error for an out-of-range shard or arithmetic overflow.
    pub fn shard_real_record_count(self, shard: usize) -> Result<usize, ScaledP3Error> {
        if shard >= self.shard_count {
            return Err(ScaledP3Error::ShardOutOfRange {
                shard,
                shard_count: self.shard_count,
            });
        }
        let start = shard
            .checked_mul(self.real_rows_per_shard)
            .ok_or(ScaledP3Error::ArithmeticOverflow("shard start"))?;
        Ok((self.database_records - start).min(self.real_rows_per_shard))
    }

    /// Validates candidates and constructs one fixed-`K` local plan per shard.
    ///
    /// Candidate validation occurs before stable deduplication and truncation.
    /// Shards without selected records therefore still receive `K` distinct
    /// local dummy windows.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid candidate, checked arithmetic failure,
    /// or canonical fixed-`K` planning failure.
    pub fn plan(self, candidates: &[RecordId]) -> Result<ScaledFetchPlan, ScaledP3Error> {
        for &candidate in candidates {
            let index =
                usize::try_from(candidate.0).map_err(|_| ScaledP3Error::CandidateOutOfRange {
                    id: candidate.0,
                    database_records: self.database_records,
                })?;
            self.map_record(index)?;
        }

        let global_capacity = self
            .database_records
            .checked_add(self.public_k)
            .ok_or(ScaledP3Error::ArithmeticOverflow("global planner capacity"))?;
        let global = FetchPlanner::new(self.database_records, global_capacity, self.public_k)?
            .plan(candidates)?;
        let mut selected_ids = Vec::with_capacity(global.selected_real_count());
        for (&id, &is_dummy) in global.physical_ids().iter().zip(global.is_dummy()) {
            if !is_dummy {
                selected_ids.push(id);
            }
        }

        let mut shard_plans = Vec::with_capacity(self.shard_count);
        for shard_index in 0..self.shard_count {
            let shard_real_count = self.shard_real_record_count(shard_index)?;
            let shard_start = shard_index
                .checked_mul(self.real_rows_per_shard)
                .ok_or(ScaledP3Error::ArithmeticOverflow("shard start"))?;
            let mut global_ids = Vec::new();
            let mut local_ids = Vec::new();

            for &global_id in &selected_ids {
                let global_index = usize::try_from(global_id.0).map_err(|_| {
                    ScaledP3Error::CandidateOutOfRange {
                        id: global_id.0,
                        database_records: self.database_records,
                    }
                })?;
                let address = self.map_record(global_index)?;
                if address.shard_index == shard_index {
                    let local_index = global_index
                        .checked_sub(shard_start)
                        .ok_or(ScaledP3Error::ArithmeticOverflow("local ID"))?;
                    global_ids.push(global_id);
                    local_ids.push(RecordId(
                        u64::try_from(local_index)
                            .map_err(|_| ScaledP3Error::ArithmeticOverflow("local ID"))?,
                    ));
                }
            }

            let local_plan =
                FetchPlanner::new(shard_real_count, PHYSICAL_WINDOWS_PER_SHARD, self.public_k)?
                    .plan(&local_ids)?;
            shard_plans.push(ScaledShardPlan {
                shard_index,
                global_ids,
                local_plan,
            });
        }

        Ok(ScaledFetchPlan {
            layout: self,
            selected_ids,
            truncated_unique_count: global.truncated_unique_count(),
            shard_plans,
        })
    }
}

impl ScaledFetchPlan {
    /// Returns the public layout bound to this plan.
    #[must_use]
    pub const fn layout(&self) -> ScaledLayout {
        self.layout
    }

    /// Returns selected real IDs in stable global candidate order.
    #[must_use]
    pub fn selected_ids(&self) -> &[RecordId] {
        &self.selected_ids
    }

    /// Returns the number of distinct valid candidates omitted after `K`.
    #[must_use]
    pub const fn truncated_unique_count(&self) -> usize {
        self.truncated_unique_count
    }

    /// Returns the number of fixed-shape shard plans.
    #[must_use]
    pub fn shard_plan_count(&self) -> usize {
        self.shard_plans.len()
    }

    /// Returns the `K` local physical IDs for one shard.
    ///
    /// # Errors
    ///
    /// Returns an error if `shard` is outside the plan.
    pub fn shard_physical_ids(&self, shard: usize) -> Result<&[RecordId], ScaledP3Error> {
        Ok(self.shard(shard)?.local_plan.physical_ids())
    }

    /// Returns selected global real IDs belonging to one shard.
    ///
    /// # Errors
    ///
    /// Returns an error if `shard` is outside the plan.
    pub fn shard_selected_global_ids(&self, shard: usize) -> Result<&[RecordId], ScaledP3Error> {
        Ok(&self.shard(shard)?.global_ids)
    }

    /// Returns the number of selected real records in one shard.
    ///
    /// # Errors
    ///
    /// Returns an error if `shard` is outside the plan.
    pub fn shard_selected_real_count(&self, shard: usize) -> Result<usize, ScaledP3Error> {
        Ok(self.shard(shard)?.local_plan.selected_real_count())
    }

    /// Returns the number of local dummy windows selected in one shard.
    ///
    /// # Errors
    ///
    /// Returns an error if `shard` is outside the plan.
    pub fn shard_dummy_count(&self, shard: usize) -> Result<usize, ScaledP3Error> {
        let local_plan = &self.shard(shard)?.local_plan;
        Ok(local_plan.public_k() - local_plan.selected_real_count())
    }

    fn shard(&self, shard: usize) -> Result<&ScaledShardPlan, ScaledP3Error> {
        let shard_plan = self
            .shard_plans
            .get(shard)
            .ok_or(ScaledP3Error::ShardOutOfRange {
                shard,
                shard_count: self.shard_plans.len(),
            })?;
        debug_assert_eq!(shard_plan.shard_index, shard);
        Ok(shard_plan)
    }
}

/// Validation and execution failures for proposal-sized P3 retrieval.
#[derive(Debug, Error)]
pub enum ScaledP3Error {
    /// Database cardinality is outside the declared experiment matrix.
    #[error("database has {actual} records; supported sizes are 1024, 2048, 4096, 8192, and 16384")]
    UnsupportedDatabaseRecords { actual: usize },
    /// Payload width is outside the declared experiment matrix.
    #[error("payload has {actual} bytes; supported widths are 256 and 1024")]
    UnsupportedPayloadBytes { actual: usize },
    /// Public `K` is outside the canonical P3 values.
    #[error("public K={actual} is unsupported")]
    UnsupportedPublicK { actual: usize },
    /// Candidate lies outside the public real-record prefix.
    #[error("candidate {id} is outside real record range 0..{database_records}")]
    CandidateOutOfRange { id: u64, database_records: usize },
    /// Requested shard lies outside the derived shard range.
    #[error("shard {shard} is outside shard range 0..{shard_count}")]
    ShardOutOfRange { shard: usize, shard_count: usize },
    /// A checked layout operation overflowed.
    #[error("checked arithmetic failed for {0}")]
    ArithmeticOverflow(&'static str),
    /// Canonical fixed-`K` planning failed.
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// The reused P0-A cryptographic kernel rejected an operation.
    #[error(transparent)]
    Cpir(#[from] CpirError),
    /// Native binary serialization or deserialization failed.
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    /// Scaling is defined only for the validated P0-A mini configuration.
    #[error("scaled P3 requires the exact validated P0-A mini configuration")]
    UnsupportedKernelConfig,
    /// Native parameter selection did not preserve the required packing shape.
    #[error(
        "unexpected kernel layout: ring={ring_dimension}, record slots={record_slots}, capacity={capacity}; expected 8192/128/64"
    )]
    UnexpectedKernelLayout {
        ring_dimension: usize,
        record_slots: usize,
        capacity: usize,
    },
    /// A source record differs from the public fixed payload width.
    #[error("record {index} has {actual} bytes; expected exactly {expected}")]
    RecordWidth {
        index: usize,
        actual: usize,
        expected: usize,
    },
    /// A Fetch plan was created for another public layout.
    #[error("plan layout mismatch: expected {expected:?}, received {actual:?}")]
    PlanLayoutMismatch {
        expected: ScaledLayout,
        actual: ScaledLayout,
    },
    /// Requested chunk plane lies outside the payload.
    #[error("chunk {chunk} is outside chunk range 0..{chunk_count}")]
    ChunkOutOfRange { chunk: usize, chunk_count: usize },
    /// Decrypted slot differs from the selected plaintext-database slot.
    #[error(
        "slot mismatch in shard {shard}, chunk {chunk}, slot {position}: expected {expected}, received {actual}"
    )]
    SlotMismatch {
        shard: usize,
        chunk: usize,
        position: usize,
        expected: u64,
        actual: u64,
    },
    /// Observed physical operation count differs from the public formula.
    #[error("observed {label} count {actual}; expected {expected}")]
    ObservedCountMismatch {
        label: &'static str,
        expected: usize,
        actual: usize,
    },
    /// One selected record did not receive every payload chunk.
    #[error("record {id} received {actual} chunks; expected {expected}")]
    MissingChunk {
        id: u64,
        expected: usize,
        actual: usize,
    },
    /// Local evidence-file cleanup or inspection failed.
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
