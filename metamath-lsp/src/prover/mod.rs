//! The proof assistant itself.
//!
//! This module includes structures used to represent proofs, proof steps, and tactics to create the proof steps.

mod context;
mod proof_step;
pub mod tactics;

use std::fmt::Display;

pub use self::context::Context;
pub use self::proof_step::ProofStep;

pub type TacticsResult<T = ProofStep> = std::result::Result<T, TacticsError>;

#[derive(Clone, Debug)]
pub enum TacticsError {
    // This is temporary. Ideally each error would have a code, which would be much lighter to report.
    Error(String),
    UnificationFailedForHyp(usize),
}

impl From<&str> for TacticsError {
    fn from(s: &str) -> Self {
        TacticsError::Error(s.to_string())
    }
}

impl Display for TacticsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TacticsError::Error(string) => f.write_str(string),
            TacticsError::UnificationFailedForHyp(_) => {
                f.write_str("Unification failed for hypothesis")
            }
        }
    }
}

/// The trait implemented by all tactics.
pub trait Tactics {
    fn get_name(&self) -> String;
    fn elaborate(&self, context: &mut Context) -> TacticsResult;
}
