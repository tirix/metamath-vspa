//! Representation of a Proof Step
use metamath_knife::formula::Formula;
use metamath_knife::formula::Label;
use std::ops::Range;

/// Identifies a proof step in a worksheet
pub type StepId = usize;

/// The type of the step: hypothesis, normal proof step of QED step.
enum StepType {
    Hyp,
    Step,
    Qed,
}

/// A Proof Step.
pub struct Step {
    /// The first byte position of this step in the proof source
    pub(crate) start: usize,
    /// The last byte position of this step in the proof source (+1)
    pub(crate) end: usize,
    /// Label of this step
    name: String,
    /// Type of this step
    step_type: StepType,
    /// Hypotheses (`None` for the "?" hypothesis)
    hyps: Vec<Option<String>>,
    /// Label (`None` for the "?" label or no label)
    label: Option<Label>,
    /// The formula for this step (Or `None` if the formula could not be parsed)
    formula: Option<Formula>,
}

