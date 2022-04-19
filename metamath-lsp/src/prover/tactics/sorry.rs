use crate::prover::{Tactics, Context, TacticsResult, ProofStep};

/// This is the default "sorry" tactics. 
/// It provides a temporary 
/// An admission of failure, or lazyness to provide a proof.
pub struct Sorry;

impl Tactics for Sorry {
    fn get_name(&self) -> String {
        "sorry".to_string()
    }

    fn elaborate(&self, context: &mut Context) -> TacticsResult {
        Ok(ProofStep::sorry(context.goal().clone()))
    }
}