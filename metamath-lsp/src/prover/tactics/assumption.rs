use crate::prover::{Context, Tactics, TacticsError, TacticsResult};

/// The assumption tactics attemps to unify the goal with all assumptions in the context.
pub struct Assumption;

impl Tactics for Assumption {
    fn get_name(&self) -> String {
        "assumption".to_string()
    }

    fn elaborate(&self, context: &mut Context) -> TacticsResult {
        for (_, step) in context.known_steps() {
            // TODO: Here, we need to unify and resolve work variables if any!
            //let mut substitutions = Substitutions::new();
            //if step.result().unify(context.goal(), &mut substitutions).is_err() { continue; }
            if step.result().eq(context.goal()) {
                return Ok(step.clone());
            }
        }
        Err(TacticsError::from("No assumption matches the goal!"))
    }
}
