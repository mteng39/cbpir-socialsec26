//! Scalar plaintext precursor tests for the P0-A fixed-window oracle.
//!
//! Begin by adding one failing example for the public interface specified in
//! `next_guided_checkpoint.md`. Run that test and confirm the expected failure
//! before adding the minimum implementation in `src/plaintext.rs`.

use cbpir_bgv::plaintext::{SelectionError, one_hot, select_plaintext};
use proptest::prelude::*;

#[test]
fn one_hot_selects_the_only_element() {
    let result = one_hot(0, 1);

    assert_eq!(result, Ok(vec![1]));
}

#[test]
fn one_hot_rejects_empty_domain() {
    let result = one_hot(0, 0);

    assert_eq!(result, Err(SelectionError::EmptyDomain));
}

#[test]
fn one_hot_rejects_index_equal_to_length() {
    let result = one_hot(3, 3);

    assert_eq!(
        result,
        Err(SelectionError::IndexOutOfBounds { index: 3, len: 3 })
    );
}

#[test]
fn one_hot_rejects_index_greater_than_length() {
    let result = one_hot(4, 3);

    assert_eq!(
        result,
        Err(SelectionError::IndexOutOfBounds { index: 4, len: 3 })
    );
}

#[test]
fn one_hot_constructs_first_middle_and_last_selectors() {
    assert_eq!(one_hot(0, 4), Ok(vec![1, 0, 0, 0]));
    assert_eq!(one_hot(2, 4), Ok(vec![0, 0, 1, 0]));
    assert_eq!(one_hot(3, 4), Ok(vec![0, 0, 0, 1]));
}

#[test]
fn select_plaintext_selects_the_only_element() {
    assert_eq!(select_plaintext(&[42], &[1]), Ok(42));
}

#[test]
fn select_plaintext_selects_first_middle_and_last_elements() {
    let database = [10, 20, 30, 40];

    assert_eq!(select_plaintext(&database, &[1, 0, 0, 0]), Ok(10));
    assert_eq!(select_plaintext(&database, &[0, 0, 1, 0]), Ok(30));
    assert_eq!(select_plaintext(&database, &[0, 0, 0, 1]), Ok(40));
}

#[test]
fn select_plaintext_handles_zero_and_duplicate_values() {
    let database = [0, 7, 7];

    assert_eq!(select_plaintext(&database, &[1, 0, 0]), Ok(0));
    assert_eq!(select_plaintext(&database, &[0, 0, 1]), Ok(7));
}

#[test]
fn select_plaintext_selects_u64_max_without_overflow() {
    assert_eq!(select_plaintext(&[1, u64::MAX], &[0, 1]), Ok(u64::MAX));
}

#[test]
fn select_plaintext_rejects_empty_inputs() {
    assert_eq!(select_plaintext(&[], &[]), Err(SelectionError::EmptyDomain));
    assert_eq!(
        select_plaintext(&[], &[1]),
        Err(SelectionError::EmptyDomain)
    );
    assert_eq!(
        select_plaintext(&[1], &[]),
        Err(SelectionError::EmptyDomain)
    );
}

#[test]
fn select_plaintext_rejects_length_mismatch_in_both_directions() {
    assert_eq!(
        select_plaintext(&[10, 20], &[1]),
        Err(SelectionError::LengthMismatch {
            database_len: 2,
            selector_len: 1,
        })
    );
    assert_eq!(
        select_plaintext(&[10], &[1, 0]),
        Err(SelectionError::LengthMismatch {
            database_len: 1,
            selector_len: 2,
        })
    );
}

#[test]
fn select_plaintext_rejects_non_binary_selector_value() {
    assert_eq!(
        select_plaintext(&[10, 20, 30], &[0, 2, 1]),
        Err(SelectionError::NonBinarySelector {
            position: 1,
            value: 2,
        })
    );
}

#[test]
fn select_plaintext_rejects_all_zero_selector() {
    assert_eq!(
        select_plaintext(&[10, 20, 30], &[0, 0, 0]),
        Err(SelectionError::InvalidSelectorWeight { weight: 0 })
    );
}

#[test]
fn select_plaintext_rejects_multiple_selected_positions() {
    assert_eq!(
        select_plaintext(&[10, 20, 30], &[1, 0, 1]),
        Err(SelectionError::InvalidSelectorWeight { weight: 2 })
    );
}

proptest! {
    #[test]
    fn generated_one_hot_selects_the_requested_database_value(
        database in prop::collection::vec(any::<u64>(), 1..65),
        raw_index in any::<usize>(),
    ) {
        let index = raw_index % database.len();
        let selector = one_hot(index, database.len()).expect("generated inputs are valid");

        prop_assert_eq!(
            select_plaintext(&database, &selector),
            Ok(database[index])
        );
    }

    #[test]
    fn generated_one_hot_has_the_requested_invariants(
        len in 1usize..65,
        raw_index in any::<usize>(),
    ) {
        let index = raw_index % len;
        let selector = one_hot(index, len).expect("generated inputs are valid");

        prop_assert_eq!(selector.len(), len);
        prop_assert!(selector.iter().all(|&value| value == 0 || value == 1));
        prop_assert_eq!(selector.iter().sum::<u64>(), 1);
        prop_assert_eq!(selector[index], 1);
        prop_assert!(selector
            .iter()
            .enumerate()
            .all(|(position, &value)| position == index || value == 0));
    }

    #[test]
    fn clearing_the_unique_one_is_rejected(
        len in 1usize..65,
        raw_index in any::<usize>(),
    ) {
        let index = raw_index % len;
        let mut selector = one_hot(index, len).expect("generated inputs are valid");
        selector[index] = 0;
        let database = vec![0; len];

        prop_assert_eq!(
            select_plaintext(&database, &selector),
            Err(SelectionError::InvalidSelectorWeight { weight: 0 })
        );
    }

    #[test]
    fn adding_a_second_one_is_rejected(
        len in 2usize..65,
        raw_index in any::<usize>(),
        raw_offset in any::<usize>(),
    ) {
        let first = raw_index % len;
        let second = (first + 1 + raw_offset % (len - 1)) % len;
        let mut selector = one_hot(first, len).expect("generated inputs are valid");
        selector[second] = 1;
        let database = vec![0; len];

        prop_assert_eq!(
            select_plaintext(&database, &selector),
            Err(SelectionError::InvalidSelectorWeight { weight: 2 })
        );
    }

    #[test]
    fn replacing_an_entry_with_a_non_binary_value_is_rejected(
        len in 1usize..65,
        raw_position in any::<usize>(),
        value in 2u64..=u64::MAX,
    ) {
        let position = raw_position % len;
        let mut selector = one_hot(position, len).expect("generated inputs are valid");
        selector[position] = value;
        let database = vec![0; len];

        prop_assert_eq!(
            select_plaintext(&database, &selector),
            Err(SelectionError::NonBinarySelector { position, value })
        );
    }
}
