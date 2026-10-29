use serde::Serialize;
use thiserror::Error;

pub const PUBLIC_K: usize = 1;
pub const FRAME_LEN: usize = 128;
pub const MAX_PAYLOAD: usize = 114;

/// Explicit P0-C Match implementation used by a P0-D pipeline.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchBackend {
    /// Frozen P0-C ciphertext--ciphertext baseline.
    #[default]
    CtCt,
    /// Server-owned plaintext-text ciphertext--plaintext variant.
    ServerTextCtPt,
}

impl MatchBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CtCt => "ct_ct",
            Self::ServerTextCtPt => "server_text_ct_pt",
        }
    }

    pub fn parse(value: &str) -> Result<Self, P0dError> {
        match value {
            "ct_ct" => Ok(Self::CtCt),
            "server_text_ct_pt" => Ok(Self::ServerTextCtPt),
            _ => Err(P0dError::Request("unknown Match backend")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct RecordId(u64);
impl RecordId {
    pub fn new(value: u64) -> Result<Self, P0dError> {
        if value == 0 {
            return Err(P0dError::Catalog("zero RecordId"));
        }
        Ok(Self(value))
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LogicalRecord {
    pub id: RecordId,
    pub searchable: Vec<u8>,
    pub payload: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
pub enum MatchMode {
    Fps,
    Rfps { randomizer: Vec<u8> },
}
#[derive(Clone, Debug, Serialize)]
pub struct MatchRequest {
    pub pattern: Vec<u8>,
    pub threshold: f64,
    pub mode: MatchMode,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReconstructedRecord {
    pub id: RecordId,
    pub payload: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum FetchTarget {
    Real { id: RecordId, physical_index: usize },
    Dummy { physical_index: usize },
}
impl FetchTarget {
    pub const fn index(self) -> usize {
        match self {
            Self::Real { physical_index, .. } | Self::Dummy { physical_index } => physical_index,
        }
    }
}

#[derive(Debug, Error)]
pub enum P0dError {
    #[error("catalog: {0}")]
    Catalog(&'static str),
    #[error("request: {0}")]
    Request(&'static str),
    #[error("Match contract: {0}")]
    MatchContract(&'static str),
    #[error("Locate: offset overflow")]
    OffsetOverflow,
    #[error("Fetch target: {0}")]
    Target(&'static str),
    #[error("Fetch frame: {0}")]
    Frame(&'static str),
    #[error("Match component: {0}")]
    MatchComponent(String),
    #[error("Fetch component: {0}")]
    FetchComponent(String),
    #[error("serialization: {0}")]
    Serialization(String),
    #[error("runtime version: {0}")]
    Runtime(String),
}
