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
        let substitutions = context
            .goal()
            .unify(&formula)
            .ok_or_else(|| TacticsError::from("Unification failed"))?;
        let mut hyp_steps = vec![];
        for (_, ess_fmla) in essentials.into_iter() {
            //let ess_sref = context.db.statement_by_label(label).ok_or(TacticsError::from("Unknown essential"))?;
            //let ess_fmla = context.db.stmt_parse_result().get_formula(&ess_sref).ok_or(TacticsError::from("Essential formula not found"))?;
            //let ess_subst = ess_fmla.unify(&formula).ok_or(TacticsError::from("Unification failed"))?;
            //check_and_extend(&mut substitutions, &ess_subst, hyp_idx)?;
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
