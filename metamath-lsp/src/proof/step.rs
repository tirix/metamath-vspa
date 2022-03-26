//! Representation of a Proof Step
use std::ops::Range;
use std::slice::Iter;

use super::worksheet::Diag;
use lazy_static::lazy_static;
use memchr::memchr;
use metamath_knife::formula::Label;
use metamath_knife::Database;
use metamath_knife::Formula;
use regex::Regex;

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
    /// Start position of this step
    //start: RopePosition<StepsInfo>,
    /// End position of this step
    //end: RopePosition<StepsInfo>,
    /// Label of this step
    name: String,
    /// Type of this step
    step_type: StepType,
    /// Hypotheses (`None` for the "?" hypothesis)
    hyps: Vec<(Option<String>, Range<usize>)>,
    /// Label (`None` for the "?" label or no label)
    label: Option<Label>,
    /// The formula for this step (Or `None` if the formula could not be parsed)
    formula: Option<Formula>,
}

impl Step {
    pub fn from_str(s: &'_ str, database: &Database) -> (Step, Vec<Diag>) {
        lazy_static! {
            static ref PROOF_LINE: Regex = Regex::new(
                r"([0-9a-z]+):([\?|0-9a-z]*)(?:,?([\?|0-9a-z]+))*:(\?|[0-9A-Za-z_\-\.]+)[ \t\n]+(.+)",
            ).expect("Malformed Regex");
        }
        let nset = database.name_result();
        let grammar = database.grammar_result();
        let _provable = grammar.provable_typecode();
        let mut diags = vec![];
        let step = if let Some(caps) = PROOF_LINE.captures(s) {
            let step_name = caps
                .get(1)
                .expect("Regex did not return right number of captures")
                .as_str();
            let mut hyps = vec![];
            for idx in 2..caps.len() - 3 {
                let capture = caps
                    .get(idx)
                    .expect("Regex did not return right number of captures");
                let hyp_name = capture.as_str();
                hyps.push((
                    if hyp_name == "?" {
                        None
                    } else {
                        Some(hyp_name.to_string())
                    },
                    capture.range(),
                ));
            }
            let capture = caps
                .get(caps.len() - 2)
                .expect("Regex did not return right number of captures");
            let label = nset
                .lookup_label(capture.as_str().as_bytes())
                .map(|l| l.atom);
            if !label.is_some() {
                diags.push(Diag::UnknownTheoremLabel(capture.range()));
            }
            let formula_string = caps
                .get(caps.len() - 1)
                .expect("Regex did not return right number of captures")
                .as_str();
            // TODO check that the formula starts with the "provable" typecode token
            let formula = match grammar.parse_string(formula_string, nset) {
                Ok(formula) => Some(formula),
                Err(diag) => {
                    diags.push(diag.into());
                    None
                }
            };
            Step {
                name: step_name.to_string(),
                step_type: StepType::Step,
                hyps,
                label,
                formula,
            }
        } else {
            let name = if let Some(offset) = memchr(b':', s.as_bytes()) {
                &s[..offset]
            } else {
                s
            };
            diags.push(Diag::UnparseableProofLine);
            Step {
                name: name.into(),
                step_type: StepType::Step,
                hyps: vec![],
                label: None,
                formula: None,
            }
        };
        (step, diags)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hyps(&self) -> Iter<(Option<String>, Range<usize>)> {
        self.hyps.iter()
    }
}
