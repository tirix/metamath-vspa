use crate::prover::{Context, Tactics, TacticsError, TacticsResult};

/// The assumption tactics attemps to unify the goal with all assumptions in the context.
pub struct Assumption;

impl Tactics for Assumption {
    fn get_name(&self) -> String {
        "assumption".to_string()
    }

    fn elaborate(&self, _context: &mut Context) -> TacticsResult {
        Err(TacticsError::from("Not implemented!"))
    }
}
