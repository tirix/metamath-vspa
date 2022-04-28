
use metamath_knife::{Label, formula::Substitutions};

use crate::prover::{Context, ProofStep, Tactics, TacticsError, TacticsResult, TacticsList};

/// The "Apply Theorem" tactics tries to apply the given theorem to the goal,
pub struct Apply{
    label: Label,
    subtactics: TacticsList,
    substitutions: Substitutions,
}

impl Apply {
    /// Creates a new "Apply Theorem" tactics for the given theorem,
    /// whereas proofs for the essential hypotheses are elaborated using the given sub-tactics.
    pub fn new(label: Label, subtactics: TacticsList) -> Self {
        Self { label, subtactics, substitutions: Substitutions::new() }
    }
}

impl Tactics for Apply {
    fn get_name(&self) -> String {
        "apply theorem".to_string()
    }

    fn elaborate(&self, context: &mut Context) -> TacticsResult {
        let (formula, essentials) = context
            .get_theorem_formulas(self.label)
            .ok_or_else(|| TacticsError::from("Unknown theorem"))?;
        let mut substitutions = Substitutions::new();
        context
            .goal()
            .unify(&formula, &mut substitutions)?;
        let mut hyp_steps = vec![];
        for (hyp_idx, (_, ess_fmla)) in essentials.into_iter().enumerate() {
            // Complete the substitutions with new work variables if needed
            ess_fmla.as_ref(&context.db.clone()).complete_substitutions(&mut substitutions, context)?;

            // Then, use the provided sub-tactics to elaborate proofs for the required hypotheses, as sub-goals
            let formula = ess_fmla.substitute(&substitutions);
            let subproof = self.subtactics[hyp_idx].elaborate(&mut context.with_goal(formula))?;
            hyp_steps.push(subproof);
        }
        Ok(ProofStep::apply(
            self.label,
            hyp_steps.into_boxed_slice(),
            context.goal().clone(),
            Box::new(substitutions),
        ))
    }
}
