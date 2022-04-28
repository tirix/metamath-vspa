//! This module handles the textual representation of proofs as ASCII files, named with the ".mmp" extension.
// TODO rename to "worksheet"
mod step;
mod worksheet;

#[cfg(test)]
mod worksheet_tests;

pub use worksheet::ProofWorksheet;
