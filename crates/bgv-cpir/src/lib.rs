//! P0-A BGV index-CPIR baseline.
//!
//! Plaintext reference modules compile by default. Enable the
//! `openfhe-backend` feature for the cryptographic `OpenFHE` implementation.

pub mod fixed_window;
pub mod plaintext;

#[cfg(feature = "openfhe-backend")]
mod backend;

#[cfg(feature = "openfhe-backend")]
pub use backend::*;
