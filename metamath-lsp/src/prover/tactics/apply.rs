
use metamath_knife::Label;

use crate::prover::{Context, ProofStep, Tactics, TacticsError, TacticsResult};

/// The "Apply Theorem" tactics tries to apply the given theorem to the goal,
/// and to match the essential hypotheses with all existing steps.
pub struct Apply(Label);

impl Apply {
    pub fn new(label: Label) -> Self {
        Self(label)
    }
}

impl Tactics for Apply {
    fn get_name(&self) -> String {
        "apply theorem".to_string()
    }

    fn elaborate(&self, context: &mut Context) -> TacticsResult {
        let (formula, essentials) = context
            .get_theorem_formulas(self.0)
            .ok_or_else(|| TacticsError::from("Unknown theorem"))?;
        let mut substitutions = context
            .goal()
            .unify(&formula)
            .ok_or_else(|| TacticsError::from("Unification failed"))?;
        let mut hyp_steps = vec![];
        for (_, ess_fmla) in essentials.into_iter() {
            // Complete the substitutions with new work variables if needed
            ess_fmla.as_ref(&context.db.clone()).complete_substitutions(&mut substitutions, context)?;
            let formula = ess_fmla.substitute(&substitutions);
            hyp_steps.push(ProofStep::sorry(formula)); // TODO, recursively try to match with existing steps rather than "sorry"
        }
        Ok(ProofStep::apply(
            self.0,
            hyp_steps.into_boxed_slice(),
            context.goal().clone(),
            substitutions,
        ))
    }
}
