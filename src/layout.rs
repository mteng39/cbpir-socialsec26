use crate::{FetchTarget, LogicalRecord, MAX_PAYLOAD, MatchMode, MatchRequest, P0dError, RecordId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct SearchSpan {
    pub id: RecordId,
    pub physical_index: usize,
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Catalog {
    records: Vec<LogicalRecord>,
    text: Vec<u8>,
    spans: Vec<SearchSpan>,
}
impl Catalog {
    pub fn new(mut records: Vec<LogicalRecord>) -> Result<Self, P0dError> {
        if records.is_empty() {
            return Err(P0dError::Catalog("empty catalog"));
        }
        records.sort_by_key(|r| r.id);
        if records.windows(2).any(|w| w[0].id == w[1].id) {
            return Err(P0dError::Catalog("duplicate RecordId"));
        }
        let mut text = Vec::new();
        let mut spans = Vec::new();
        for (index, r) in records.iter().enumerate() {
            if r.searchable.is_empty() {
                return Err(P0dError::Catalog("empty searchable field"));
            }
            if r.payload.len() > MAX_PAYLOAD {
                return Err(P0dError::Catalog("payload exceeds 114 bytes"));
            }
            if index > 0 {
                text.push(0);
            }
            let start = text.len();
            let end = start
                .checked_add(r.searchable.len())
                .ok_or(P0dError::OffsetOverflow)?;
            spans.push(SearchSpan {
                id: r.id,
                physical_index: index,
                start,
                end,
            });
            text.extend_from_slice(&r.searchable);
        }
        Ok(Self {
            records,
            text,
            spans,
        })
    }
    pub fn records(&self) -> &[LogicalRecord] {
        &self.records
    }
    pub fn text(&self) -> &[u8] {
        &self.text
    }
    pub fn spans(&self) -> &[SearchSpan] {
        &self.spans
    }
    pub fn dummy_index(&self) -> usize {
        self.records.len()
    }
    pub fn validate(&self, q: &MatchRequest) -> Result<(), P0dError> {
        if q.pattern.is_empty() || q.pattern.len() > 64 || q.pattern.len() > self.text.len() {
            return Err(P0dError::Request("pattern length outside declared domain"));
        }
        if !q.threshold.is_finite() || q.threshold < 0.0 {
            return Err(P0dError::Request(
                "threshold must be finite and nonnegative",
            ));
        }
        if let MatchMode::Rfps { randomizer } = &q.mode {
            if randomizer.len() != q.pattern.len() {
                return Err(P0dError::Request("randomizer length mismatch"));
            }
            if randomizer.iter().any(|x| !(1..=32).contains(x)) {
                return Err(P0dError::Request("randomizer outside 1..=32"));
            }
        }
        Ok(())
    }
    pub fn validate_target(&self, target: FetchTarget) -> Result<(), P0dError> {
        match target {
            FetchTarget::Dummy { physical_index } if physical_index == self.dummy_index() => Ok(()),
            FetchTarget::Real { id, physical_index }
                if self.records.get(physical_index).is_some_and(|r| r.id == id) =>
            {
                Ok(())
            }
            _ => Err(P0dError::Target("ID/index or dummy/catalog mismatch")),
        }
    }
}
