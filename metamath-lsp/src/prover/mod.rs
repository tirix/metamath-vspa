//! The proof assistant itself.
//! 
//! This module includes structures used to represent proofs, proof steps, and tactics to create the proof steps.

mod context;
mod proof_step;
pub mod tactics;

use std::fmt::Display;

use metamath_knife::formula::Substitutions;

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
            TacticsError::UnificationFailedForHyp(hyp_idx) => f.write_fmt(format_args!("Unification failed for hypothesis {}", hyp_idx)),
        }
    }
}

/// The trait implemented by all tactics.
pub trait Tactics {
    fn get_name(&self) -> String;
    fn elaborate(&self, context: &mut Context) -> TacticsResult;
}

// TODO - move this to metamath_knife!!
pub fn check_and_extend(
    s1: &mut Substitutions,
    s2: &Substitutions,
    hyp_idx: usize,
) -> Result<(), TacticsError> {
    for (&label, f1) in s1.into_iter() {
        if let Some(f2) = s2.get(label) {
            if f1 != f2 {
                return Err(TacticsError::UnificationFailedForHyp(hyp_idx));
            }
        }
    }
    s1.extend(s2);
    Ok(())
}
