//! The proof assistant itself.
//!
//! This module includes structures used to represent proofs, proof steps, and tactics to create the proof steps.

mod context;
mod proof_step;
pub mod tactics;

use std::fmt::Display;

use metamath_knife::formula::UnificationError;

pub use self::context::Context;
pub use self::proof_step::ProofStep;

pub type TacticsResult<T = ProofStep> = std::result::Result<T, TacticsError>;

#[derive(Clone, Debug)]
pub enum TacticsError {
    // This is temporary. Ideally each error would have a code, which would be much lighter to report.
    Error(String),
    UnificationFailed(UnificationError),
    UnificationFailedForHyp(usize, UnificationError),
}

impl From<&str> for TacticsError {
    fn from(s: &str) -> Self {
        TacticsError::Error(s.to_string())
    }
}

impl From<UnificationError> for TacticsError {
    fn from(e: UnificationError) -> Self {
        TacticsError::UnificationFailed(e)
    }
}

impl From<(usize, UnificationError)> for TacticsError {
    fn from(e: (usize, UnificationError)) -> Self {
        TacticsError::UnificationFailedForHyp(e.0, e.1)
    }
}

impl Display for TacticsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TacticsError::Error(string) => f.write_str(string),
            TacticsError::UnificationFailed(_) => {
                f.write_str("Unification failed")
            }
            TacticsError::UnificationFailedForHyp(_, _) => {
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
