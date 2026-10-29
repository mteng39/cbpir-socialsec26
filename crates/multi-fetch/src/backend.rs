use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use cbpir_bgv::{
    BgvCpir, BgvCpirConfig, CpirError, EncryptedQuery, EncryptedResponse, PreparedDatabase,
    SerializationError, SerializedBundle,
};
use thiserror::Error;

use crate::{FetchError, FetchPlan, FetchPlanner, FetchedRecord, discard_dummy_responses};

/// Setup costs for one canonical P3 context and packed database.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct P3SetupTimings {
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
    pub database_assembly_and_encode_ns: u64,
}

/// Online costs for one query, one answer, and one client reconstruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct P3FetchTimings {
    pub query_encode_encrypt_ns: u64,
    pub server_multiply_ns: u64,
    pub client_decrypt_decode_ns: u64,
}

/// Records and full decrypted slot evidence from one P3 exchange.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P3FetchResult {
    pub records: Vec<FetchedRecord>,
    pub decrypted_slots: Vec<u64>,
    pub timings: P3FetchTimings,
}

/// Canonical P3 orchestration over one P0-A BGV context and packed database.
pub struct P3BgvBackend {
    kernel: BgvCpir,
    database: PreparedDatabase,
    planner: FetchPlanner,
}

impl P3BgvBackend {
    /// Creates one exact-width real prefix followed by `K` zero dummy windows.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid HE parameters, unsupported/capacity-invalid
    /// `K`, an empty real database, or any record not exactly one window wide.
    pub fn new(
        records: &[Vec<u8>],
        public_k: usize,
        config: BgvCpirConfig,
    ) -> Result<(Self, P3SetupTimings), P3BackendError> {
        if records.is_empty() {
            return Err(FetchError::EmptyRealDatabase.into());
        }

        let (kernel, setup) = BgvCpir::new_with_setup_timings(config)?;
        let planner = FetchPlanner::new(records.len(), kernel.capacity(), public_k)?;
        for (index, record) in records.iter().enumerate() {
            if record.len() != kernel.record_slots() {
                return Err(P3BackendError::RecordWidth {
                    index,
                    actual: record.len(),
                    expected: kernel.record_slots(),
                });
            }
        }

        let database_started = Instant::now();
        let mut physical_records = records.to_vec();
        physical_records.extend((0..public_k).map(|_| vec![0_u8; kernel.record_slots()]));
        let database = kernel.prepare_database(&physical_records)?;
        let database_assembly_and_encode_ns = elapsed_ns(database_started);

        Ok((
            Self {
                kernel,
                database,
                planner,
            },
            P3SetupTimings {
                context_generation_ns: setup.context_generation_ns,
                key_generation_ns: setup.key_generation_ns,
                database_assembly_and_encode_ns,
            },
        ))
    }

    #[must_use]
    pub const fn planner(&self) -> FetchPlanner {
        self.planner
    }

    #[must_use]
    pub const fn kernel(&self) -> &BgvCpir {
        &self.kernel
    }

    #[must_use]
    pub const fn database(&self) -> &PreparedDatabase {
        &self.database
    }

    /// Answers one ordinary P0-A query against P3's matched packed database.
    ///
    /// This narrow hook exists for the repeated-P0-A comparison path. `OpenFHE`
    /// 1.5 requires mutable access to the plaintext handle even though the
    /// application-level database values remain unchanged.
    #[must_use]
    pub fn answer_p0a_query(&mut self, query: &EncryptedQuery) -> EncryptedResponse {
        self.kernel.answer(&mut self.database, query)
    }

    /// Executes one encrypted multi-window query and reconstructs real records.
    ///
    /// # Errors
    ///
    /// Returns an error for a plan from another layout or any P0-A
    /// encryption, decryption, or decode failure.
    pub fn fetch(&mut self, plan: &FetchPlan) -> Result<P3FetchResult, P3BackendError> {
        let indices = self.plan_indices(plan)?;
        let (_query, response, query_encode_encrypt_ns, server_multiply_ns) =
            self.encrypt_and_answer(&indices)?;

        let client_started = Instant::now();
        let slots = self.kernel.decrypt_slots(&response)?;
        let records = self.decode_plan_slots_for_indices(plan, &indices, &slots)?;
        let client_decrypt_decode_ns = elapsed_ns(client_started);

        Ok(P3FetchResult {
            records,
            decrypted_slots: slots,
            timings: P3FetchTimings {
                query_encode_encrypt_ns,
                server_multiply_ns,
                client_decrypt_decode_ns,
            },
        })
    }

    /// Executes and serializes one encrypted multi-window exchange.
    ///
    /// Serialization uses the same query and response objects measured by the
    /// returned online timings. Serialization time itself is not included.
    /// The bundle contains the client secret key and therefore must be kept as
    /// controlled experiment evidence, never sent to the server.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid plan, cryptographic failure, decode
    /// failure, filesystem failure, or native serialization failure.
    pub fn fetch_and_serialize(
        &mut self,
        plan: &FetchPlan,
        directory: &Path,
    ) -> Result<(P3FetchResult, SerializedBundle), P3BackendError> {
        let indices = self.plan_indices(plan)?;
        let (query, response, query_encode_encrypt_ns, server_multiply_ns) =
            self.encrypt_and_answer(&indices)?;
        let bundle = self
            .kernel
            .serialize_binary_bundle(directory, &query, &response)?;

        let client_started = Instant::now();
        let slots = self.kernel.decrypt_slots(&response)?;
        let records = self.decode_plan_slots_for_indices(plan, &indices, &slots)?;
        let client_decrypt_decode_ns = elapsed_ns(client_started);

        Ok((
            P3FetchResult {
                records,
                decrypted_slots: slots,
                timings: P3FetchTimings {
                    query_encode_encrypt_ns,
                    server_multiply_ns,
                    client_decrypt_decode_ns,
                },
            },
            bundle,
        ))
    }

    /// Decodes records from a full decrypted slot vector using client-known
    /// plan order and dummy flags.
    ///
    /// # Errors
    ///
    /// Returns an error for a foreign/malformed plan or malformed slot data.
    pub fn decode_plan_slots(
        &self,
        plan: &FetchPlan,
        slots: &[u64],
    ) -> Result<Vec<FetchedRecord>, P3BackendError> {
        let indices = self.plan_indices(plan)?;
        self.decode_plan_slots_for_indices(plan, &indices, slots)
    }

    fn plan_indices(&self, plan: &FetchPlan) -> Result<Vec<usize>, P3BackendError> {
        self.validate_plan_layout(plan)?;
        plan.physical_ids()
            .iter()
            .map(|id| {
                usize::try_from(id.0)
                    .map_err(|_| P3BackendError::PhysicalIdNotRepresentable { id: id.0 })
            })
            .collect()
    }

    fn encrypt_and_answer(
        &mut self,
        indices: &[usize],
    ) -> Result<(EncryptedQuery, EncryptedResponse, u64, u64), P3BackendError> {
        let query_started = Instant::now();
        let query = self.kernel.encrypt_window_query(indices)?;
        let query_encode_encrypt_ns = elapsed_ns(query_started);

        let server_started = Instant::now();
        let response = self.kernel.answer(&mut self.database, &query);
        let server_multiply_ns = elapsed_ns(server_started);

        Ok((query, response, query_encode_encrypt_ns, server_multiply_ns))
    }

    fn decode_plan_slots_for_indices(
        &self,
        plan: &FetchPlan,
        indices: &[usize],
        slots: &[u64],
    ) -> Result<Vec<FetchedRecord>, P3BackendError> {
        let physical_records = indices
            .iter()
            .map(|&index| {
                self.kernel
                    .decode_fixed_window(slots, index, self.kernel.record_slots())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(discard_dummy_responses(plan, &physical_records)?)
    }

    fn validate_plan_layout(&self, plan: &FetchPlan) -> Result<(), P3BackendError> {
        if plan.public_k() != self.planner.public_k()
            || plan.real_record_count() != self.planner.real_record_count()
            || plan.physical_capacity() != self.planner.physical_capacity()
        {
            return Err(P3BackendError::PlanLayoutMismatch {
                expected_real_records: self.planner.real_record_count(),
                actual_real_records: plan.real_record_count(),
                expected_physical_capacity: self.planner.physical_capacity(),
                actual_physical_capacity: plan.physical_capacity(),
                expected_public_k: self.planner.public_k(),
                actual_public_k: plan.public_k(),
            });
        }
        if plan.physical_ids().len() != self.planner.public_k()
            || plan.is_dummy().len() != self.planner.public_k()
        {
            return Err(P3BackendError::PlanShape {
                expected: self.planner.public_k(),
                physical_ids: plan.physical_ids().len(),
                dummy_flags: plan.is_dummy().len(),
            });
        }

        let mut seen = HashSet::with_capacity(plan.public_k());
        for (&id, &is_dummy) in plan.physical_ids().iter().zip(plan.is_dummy()) {
            let index = usize::try_from(id.0)
                .map_err(|_| P3BackendError::PhysicalIdNotRepresentable { id: id.0 })?;
            if index >= self.planner.physical_capacity() {
                return Err(P3BackendError::PhysicalIdOutOfRange {
                    id: id.0,
                    capacity: self.planner.physical_capacity(),
                });
            }
            if !seen.insert(index) {
                return Err(P3BackendError::DuplicatePhysicalId { id: id.0 });
            }
            let expected_dummy = index >= self.planner.real_record_count();
            if is_dummy != expected_dummy {
                return Err(P3BackendError::DummyFlagMismatch {
                    id: id.0,
                    expected_dummy,
                    actual_dummy: is_dummy,
                });
            }
        }
        Ok(())
    }
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Failures in canonical P3 setup or one-query Fetch orchestration.
#[derive(Debug, Error)]
pub enum P3BackendError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error(transparent)]
    Cpir(#[from] CpirError),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    #[error("record {index} has {actual} bytes; canonical window requires exactly {expected}")]
    RecordWidth {
        index: usize,
        actual: usize,
        expected: usize,
    },
    #[error(
        "plan layout mismatch: expected real={expected_real_records}, physical={expected_physical_capacity}, K={expected_public_k}; received real={actual_real_records}, physical={actual_physical_capacity}, K={actual_public_k}"
    )]
    PlanLayoutMismatch {
        expected_real_records: usize,
        actual_real_records: usize,
        expected_physical_capacity: usize,
        actual_physical_capacity: usize,
        expected_public_k: usize,
        actual_public_k: usize,
    },
    #[error(
        "plan shape mismatch: expected {expected}; received {physical_ids} IDs and {dummy_flags} flags"
    )]
    PlanShape {
        expected: usize,
        physical_ids: usize,
        dummy_flags: usize,
    },
    #[error("physical ID {id} cannot be represented locally")]
    PhysicalIdNotRepresentable { id: u64 },
    #[error("physical ID {id} is outside capacity {capacity}")]
    PhysicalIdOutOfRange { id: u64, capacity: usize },
    #[error("physical ID {id} appears more than once")]
    DuplicatePhysicalId { id: u64 },
    #[error(
        "physical ID {id} dummy flag mismatch: expected {expected_dummy}, received {actual_dummy}"
    )]
    DummyFlagMismatch {
        id: u64,
        expected_dummy: bool,
        actual_dummy: bool,
    },
}
