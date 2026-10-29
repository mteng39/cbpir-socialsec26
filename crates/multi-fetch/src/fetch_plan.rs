use std::collections::HashSet;
use std::fmt;

/// Public retrieval capacities validated by canonical P3.
pub const SUPPORTED_PUBLIC_K: [usize; 4] = [1, 4, 8, 16];

/// Stable proposal-level record identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecordId(pub u64);

/// Validated request containing exactly public `K` distinct physical IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchPlan {
    physical_ids: Vec<RecordId>,
    is_dummy: Vec<bool>,
    selected_real_count: usize,
    truncated_unique_count: usize,
    public_k: usize,
    real_record_count: usize,
    physical_capacity: usize,
}

impl FetchPlan {
    #[must_use]
    pub fn physical_ids(&self) -> &[RecordId] {
        &self.physical_ids
    }

    #[must_use]
    pub fn is_dummy(&self) -> &[bool] {
        &self.is_dummy
    }

    #[must_use]
    pub const fn selected_real_count(&self) -> usize {
        self.selected_real_count
    }

    #[must_use]
    pub const fn truncated_unique_count(&self) -> usize {
        self.truncated_unique_count
    }

    #[must_use]
    pub const fn public_k(&self) -> usize {
        self.public_k
    }

    #[must_use]
    pub const fn real_record_count(&self) -> usize {
        self.real_record_count
    }

    #[must_use]
    pub const fn physical_capacity(&self) -> usize {
        self.physical_capacity
    }
}

/// Public database layout used to turn candidate IDs into physical windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FetchPlanner {
    real_record_count: usize,
    physical_capacity: usize,
    public_k: usize,
}

impl FetchPlanner {
    /// Constructs a layout with `K` reserved distinct dummy windows.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty database, unsupported `K`, arithmetic
    /// overflow, or insufficient physical capacity.
    pub const fn new(
        real_record_count: usize,
        physical_capacity: usize,
        public_k: usize,
    ) -> Result<Self, FetchError> {
        if real_record_count == 0 {
            return Err(FetchError::EmptyRealDatabase);
        }
        if !is_supported_public_k(public_k) {
            return Err(FetchError::UnsupportedPublicK { public_k });
        }
        let Some(required) = real_record_count.checked_add(public_k) else {
            return Err(FetchError::InsufficientPhysicalCapacity {
                real_record_count,
                public_k,
                physical_capacity,
            });
        };
        if required > physical_capacity {
            return Err(FetchError::InsufficientPhysicalCapacity {
                real_record_count,
                public_k,
                physical_capacity,
            });
        }
        Ok(Self {
            real_record_count,
            physical_capacity,
            public_k,
        })
    }

    #[must_use]
    pub const fn real_record_count(self) -> usize {
        self.real_record_count
    }

    #[must_use]
    pub const fn physical_capacity(self) -> usize {
        self.physical_capacity
    }

    #[must_use]
    pub const fn public_k(self) -> usize {
        self.public_k
    }

    /// Validates all IDs, stable-deduplicates, truncates, then pads to `K`.
    ///
    /// # Errors
    ///
    /// Returns an error if any candidate cannot be represented locally or is
    /// outside the real-record prefix. Validation always precedes truncation.
    pub fn plan(self, candidates: &[RecordId]) -> Result<FetchPlan, FetchError> {
        for &id in candidates {
            let index =
                usize::try_from(id.0).map_err(|_| FetchError::RecordIdNotRepresentable { id })?;
            if index >= self.real_record_count {
                return Err(FetchError::CandidateOutOfRange {
                    id,
                    real_record_count: self.real_record_count,
                });
            }
        }

        let mut seen = HashSet::with_capacity(candidates.len());
        let unique = candidates
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .collect::<Vec<_>>();
        let selected_real_count = unique.len().min(self.public_k);
        let truncated_unique_count = unique.len() - selected_real_count;
        let mut physical_ids = unique.into_iter().take(self.public_k).collect::<Vec<_>>();
        let mut is_dummy = vec![false; selected_real_count];

        for dummy_offset in 0..(self.public_k - selected_real_count) {
            let index = self.real_record_count + dummy_offset;
            let id = u64::try_from(index)
                .map(RecordId)
                .map_err(|_| FetchError::PhysicalIdNotRepresentable { index })?;
            physical_ids.push(id);
            is_dummy.push(true);
        }

        Ok(FetchPlan {
            physical_ids,
            is_dummy,
            selected_real_count,
            truncated_unique_count,
            public_k: self.public_k,
            real_record_count: self.real_record_count,
            physical_capacity: self.physical_capacity,
        })
    }
}

const fn is_supported_public_k(public_k: usize) -> bool {
    matches!(public_k, 1 | 4 | 8 | 16)
}

/// One real record retained after client-side dummy removal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchedRecord {
    id: RecordId,
    bytes: Vec<u8>,
}

impl FetchedRecord {
    pub(crate) fn from_parts(id: RecordId, bytes: Vec<u8>) -> Self {
        Self { id, bytes }
    }

    #[must_use]
    pub const fn id(&self) -> RecordId {
        self.id
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Removes dummy responses using only client-held plan metadata.
///
/// # Errors
///
/// Returns an error unless response count equals public `K`.
pub fn discard_dummy_responses(
    plan: &FetchPlan,
    responses: &[Vec<u8>],
) -> Result<Vec<FetchedRecord>, FetchError> {
    if responses.len() != plan.public_k {
        return Err(FetchError::ResponseCount {
            expected: plan.public_k,
            actual: responses.len(),
        });
    }

    Ok(plan
        .physical_ids
        .iter()
        .zip(&plan.is_dummy)
        .zip(responses)
        .filter(|&((_, is_dummy), _)| !is_dummy)
        .map(|((&id, _), bytes)| FetchedRecord {
            id,
            bytes: bytes.clone(),
        })
        .collect())
}

/// Validation errors at the stable-ID planning boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FetchError {
    EmptyRealDatabase,
    UnsupportedPublicK {
        public_k: usize,
    },
    InsufficientPhysicalCapacity {
        real_record_count: usize,
        public_k: usize,
        physical_capacity: usize,
    },
    RecordIdNotRepresentable {
        id: RecordId,
    },
    PhysicalIdNotRepresentable {
        index: usize,
    },
    CandidateOutOfRange {
        id: RecordId,
        real_record_count: usize,
    },
    ResponseCount {
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRealDatabase => formatter.write_str("real database must not be empty"),
            Self::UnsupportedPublicK { public_k } => {
                write!(formatter, "public K={public_k} is not supported")
            }
            Self::InsufficientPhysicalCapacity {
                real_record_count,
                public_k,
                physical_capacity,
            } => write!(
                formatter,
                "{real_record_count} real records plus K={public_k} dummy windows exceed physical capacity {physical_capacity}"
            ),
            Self::RecordIdNotRepresentable { id } => {
                write!(
                    formatter,
                    "record ID {} cannot be represented locally",
                    id.0
                )
            }
            Self::PhysicalIdNotRepresentable { index } => {
                write!(formatter, "physical index {index} cannot be encoded as u64")
            }
            Self::CandidateOutOfRange {
                id,
                real_record_count,
            } => write!(
                formatter,
                "candidate {} is outside real record range 0..{}",
                id.0, real_record_count
            ),
            Self::ResponseCount { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} responses, received {actual}"
                )
            }
        }
    }
}

impl std::error::Error for FetchError {}
