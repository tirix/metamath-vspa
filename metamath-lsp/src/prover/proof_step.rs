use metamath_knife::formula::Substitutions;
use metamath_knife::proof::ProofTreeArray;
use metamath_knife::Formula;
use metamath_knife::Label;

use super::Context;

#[derive(Clone, Debug)]
/// One step in a proof
pub enum ProofStep {
    Apply {
        apply: Label,
        apply_on: Box<[ProofStep]>,
        result: Formula,
        substitutions: Box<Substitutions>,
    },
    Hyp {
        label: Label,
        result: Formula,
    },
    Sorry {
        result: Formula,
    },
}

impl ProofStep {
    pub fn apply(
        apply: Label,
        apply_on: Box<[ProofStep]>,
        result: Formula,
        substitutions: Box<Substitutions>,
    ) -> Self {
        ProofStep::Apply {
            apply,
            apply_on,
            result,
            substitutions,
        }
    }

    pub fn sorry(result: Formula) -> Self {
        ProofStep::Sorry { result }
    }

    pub fn hyp(label: Label, result: Formula) -> Self {
        ProofStep::Hyp { label, result }
    }

    pub fn result(&self) -> &Formula {
        match self {
            ProofStep::Apply { result: r, .. } => r,
            ProofStep::Hyp { result: r, .. } => r,
            ProofStep::Sorry { result: r, .. } => r,
        }
    }

    fn add_to_proof_tree_array(
        &self,
        stack_buffer: &mut Vec<u8>,
        arr: &mut ProofTreeArray,
        context: &Context,
    ) -> Option<usize> {
        match self {
            ProofStep::Apply {
                apply,
                apply_on,
                result,
                substitutions,
            } => {
                let hyps = apply_on
                    .iter()
                    .filter_map(|step| step.add_to_proof_tree_array(stack_buffer, arr, context))
                    .collect();
                context.build_proof_step(
                    *apply,
                    result.clone(),
                    hyps,
                    substitutions,
                    stack_buffer,
                    arr,
                )
            }
            ProofStep::Hyp { label, result } => {
                context.build_proof_hyp(*label, result.clone(), stack_buffer, arr)
            }
            ProofStep::Sorry { .. } => None,
        }
    }

    pub fn as_proof_tree_array(&self, context: &Context) -> ProofTreeArray {
        let mut arr = ProofTreeArray::default();
        let mut stack_buffer = vec![];
        arr.qed = self
            .add_to_proof_tree_array(&mut stack_buffer, &mut arr, context)
            .unwrap();
        arr
    }
}
