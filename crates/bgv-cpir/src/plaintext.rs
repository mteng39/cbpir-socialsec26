//! Backend-neutral scalar selection oracles retained as a P0-A precursor.
//!
//! This module models scalar one-hot selection before fixed-width record
//! packing or homomorphic encryption is introduced.

/// Validation failures for plaintext selector construction and evaluation.
#[derive(Debug, PartialEq, Eq)]
pub enum SelectionError {
    /// The selector domain or database is empty.
    EmptyDomain,
    /// The requested index is outside the selector domain.
    IndexOutOfBounds { index: usize, len: usize },
    /// The database and selector contain different numbers of elements.
    LengthMismatch {
        database_len: usize,
        selector_len: usize,
    },
    /// A selector entry is neither zero nor one.
    NonBinarySelector { position: usize, value: u64 },
    /// The selector does not contain exactly one selected position.
    InvalidSelectorWeight { weight: usize },
}

/// Constructs a binary selector with one at `index` and zero elsewhere.
///
/// # Errors
///
/// Returns [`SelectionError::EmptyDomain`] when `len` is zero and
/// [`SelectionError::IndexOutOfBounds`] when `index` is not less than `len`.
pub fn one_hot(index: usize, len: usize) -> Result<Vec<u64>, SelectionError> {
    if len == 0 {
        return Err(SelectionError::EmptyDomain);
    }
    if index >= len {
        return Err(SelectionError::IndexOutOfBounds { index, len });
    }

    let mut selector = vec![0; len];
    selector[index] = 1;
    Ok(selector)
}

/// Selects one database value using a checked binary selector.
///
/// The selected value is returned directly rather than through a general dot
/// product, so selecting [`u64::MAX`] cannot overflow.
///
/// # Errors
///
/// Returns a [`SelectionError`] when either input is empty, the lengths differ,
/// an entry is non-binary, or the selector's Hamming weight is not exactly one.
pub fn select_plaintext(database: &[u64], selector: &[u64]) -> Result<u64, SelectionError> {
    if database.is_empty() || selector.is_empty() {
        return Err(SelectionError::EmptyDomain);
    }
    if database.len() != selector.len() {
        return Err(SelectionError::LengthMismatch {
            database_len: database.len(),
            selector_len: selector.len(),
        });
    }
    let mut selected_value = 0;
    let mut weight = 0;
    for (position, &selector_value) in selector.iter().enumerate() {
        match selector_value {
            0 => {}
            1 => {
                selected_value = database[position];
                weight += 1;
            }
            value => {
                return Err(SelectionError::NonBinarySelector { position, value });
            }
        }
    }
    if weight != 1 {
        return Err(SelectionError::InvalidSelectorWeight { weight });
    }
    Ok(selected_value)
}
