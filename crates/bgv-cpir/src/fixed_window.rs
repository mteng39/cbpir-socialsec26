//! Plaintext oracle for the audited fixed-window SIMD masking protocol.
//!
//! The prior Go implementation stores one record byte per SIMD slot, rounds
//! the record window to a multiple of eight slots, masks the selected window,
//! and lets the client decode that already-known window.

/// Validation failures in the fixed-window plaintext oracle.
#[derive(Debug, PartialEq, Eq)]
pub enum FixedWindowError {
    /// Retained for source compatibility. Empty selectors now produce an
    /// all-zero slot vector.
    EmptySelection,
    /// No records were supplied.
    EmptyDatabase,
    /// The SIMD slot domain has zero capacity.
    EmptySlotDomain,
    /// The fixed record-window width is zero.
    ZeroWindowWidth,
    /// The packed records do not fit in the slot domain.
    CapacityExceeded { required: usize, available: usize },
    /// The selected record index is outside the database.
    IndexOutOfBounds { index: usize, record_count: usize },
    /// A record index appears more than once in a binary selector.
    DuplicateIndex { index: usize },
    /// Database and selector slot domains differ.
    LengthMismatch {
        database_slots: usize,
        selector_slots: usize,
    },
    /// A slot value cannot be decoded as one byte.
    NonByteSlot { position: usize, value: u64 },
}

/// Builds a binary selector for several complete fixed record windows.
///
/// This repeats the audited HLF-PIR window-fill operation for each distinct
/// record index while retaining one full logical-slot vector.
///
/// # Errors
///
/// An empty selection returns an all-zero slot vector. Returns an error for an
/// invalid layout, out-of-range index, duplicate index, or database layout that
/// exceeds the slot domain.
pub fn multi_window_selector(
    indices: &[usize],
    record_count: usize,
    slots_per_record: usize,
    slot_count: usize,
) -> Result<Vec<u64>, FixedWindowError> {
    if slot_count == 0 {
        return Err(FixedWindowError::EmptySlotDomain);
    }
    if slots_per_record == 0 {
        return Err(FixedWindowError::ZeroWindowWidth);
    }
    let required = record_count.saturating_mul(slots_per_record);
    if required > slot_count {
        return Err(FixedWindowError::CapacityExceeded {
            required,
            available: slot_count,
        });
    }

    let mut selector = vec![0; slot_count];
    for &index in indices {
        if index >= record_count {
            return Err(FixedWindowError::IndexOutOfBounds {
                index,
                record_count,
            });
        }
        let start = index * slots_per_record;
        if selector[start] != 0 {
            return Err(FixedWindowError::DuplicateIndex { index });
        }
        selector[start..start + slots_per_record].fill(1);
    }
    Ok(selector)
}

/// A query-independent plaintext database encoded as one byte per SIMD slot.
#[derive(Debug, PartialEq, Eq)]
pub struct PackedDatabase {
    slots: Vec<u64>,
    record_count: usize,
    slots_per_record: usize,
}

impl PackedDatabase {
    /// Returns the complete SIMD slot vector, including zero padding.
    #[must_use]
    pub fn slots(&self) -> &[u64] {
        &self.slots
    }

    /// Returns the number of packed records.
    #[must_use]
    pub const fn record_count(&self) -> usize {
        self.record_count
    }

    /// Returns the fixed record-window width in slots.
    #[must_use]
    pub const fn slots_per_record(&self) -> usize {
        self.slots_per_record
    }
}

/// Packs records into fixed windows using one byte per slot.
///
/// The window width is the longest record length rounded up to a multiple of
/// eight slots, matching the audited Go implementation.
///
/// # Errors
///
/// Returns an error when the record set or slot domain is empty, or when the
/// fixed windows exceed `slot_count`.
pub fn pack_records(
    records: &[&[u8]],
    slot_count: usize,
) -> Result<PackedDatabase, FixedWindowError> {
    if records.is_empty() {
        return Err(FixedWindowError::EmptyDatabase);
    }
    if slot_count == 0 {
        return Err(FixedWindowError::EmptySlotDomain);
    }

    let max_record_len = records.iter().map(|record| record.len()).max().unwrap_or(0);
    let slots_per_record = max_record_len.div_ceil(8) * 8;
    if slots_per_record == 0 {
        return Err(FixedWindowError::ZeroWindowWidth);
    }
    let required = records.len().saturating_mul(slots_per_record);
    if required > slot_count {
        return Err(FixedWindowError::CapacityExceeded {
            required,
            available: slot_count,
        });
    }

    let mut slots = vec![0; slot_count];
    for (record_index, record) in records.iter().enumerate() {
        let start = record_index * slots_per_record;
        for (offset, &byte) in record.iter().enumerate() {
            slots[start + offset] = u64::from(byte);
        }
    }

    Ok(PackedDatabase {
        slots,
        record_count: records.len(),
        slots_per_record,
    })
}

/// Builds the audited multi-hot selector for one fixed record window.
///
/// # Errors
///
/// Returns an error for an empty slot domain, an invalid record index, or a
/// database layout that exceeds the slot domain.
pub fn window_selector(
    index: usize,
    record_count: usize,
    slots_per_record: usize,
    slot_count: usize,
) -> Result<Vec<u64>, FixedWindowError> {
    multi_window_selector(&[index], record_count, slots_per_record, slot_count)
}

/// Applies the server's slotwise plaintext masking oracle.
///
/// # Errors
///
/// Returns an error when database and selector slot domains differ.
pub fn mask_slots(database: &[u64], selector: &[u64]) -> Result<Vec<u64>, FixedWindowError> {
    if database.len() != selector.len() {
        return Err(FixedWindowError::LengthMismatch {
            database_slots: database.len(),
            selector_slots: selector.len(),
        });
    }
    Ok(database
        .iter()
        .zip(selector)
        .map(|(&database_slot, &selector_slot)| database_slot * selector_slot)
        .collect())
}

/// Decodes the known fixed window, stopping at its first zero slot.
///
/// # Errors
///
/// Returns an error if the window is out of bounds or contains a non-byte
/// value before its zero terminator.
pub fn decode_window(
    response: &[u64],
    index: usize,
    slots_per_record: usize,
) -> Result<Vec<u8>, FixedWindowError> {
    if slots_per_record == 0 {
        return Err(FixedWindowError::ZeroWindowWidth);
    }
    let start = index.saturating_mul(slots_per_record);
    let end = start.saturating_add(slots_per_record);
    if end > response.len() {
        return Err(FixedWindowError::IndexOutOfBounds {
            index,
            record_count: response.len() / slots_per_record.max(1),
        });
    }

    let mut decoded = Vec::new();
    for (position, &value) in response[start..end].iter().enumerate() {
        if value == 0 {
            break;
        }
        let byte = u8::try_from(value).map_err(|_| FixedWindowError::NonByteSlot {
            position: start + position,
            value,
        })?;
        decoded.push(byte);
    }
    Ok(decoded)
}
