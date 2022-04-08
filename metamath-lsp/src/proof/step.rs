//! Representation of a Proof Step
use std::slice::Iter;

use super::worksheet::{ Diag, Span };
use lazy_static::lazy_static;
use memchr::memchr;
use metamath_knife::formula::Label;
use metamath_knife::Database;
use metamath_knife::Formula;
use regex::Regex;

/// The type of the step: hypothesis, normal proof step of QED step.
#[derive(Clone, Debug)]
enum StepType {
    Hyp,
    Step,
    Qed,
}

/// A Proof Step.
#[derive(Clone, Debug)]
pub struct Step {
    /// Label of this step
    label_span: Span,
    /// Type of this step
    step_type: StepType,
    /// Hypotheses (`None` for the "?" hypothesis)
    hyps: Vec<Option<Span>>,
    /// Label (`None` for the "?" label or no label) of the theorem applied
    label: Option<Label>,
    /// The span where the formula can be found
    formula_span: Option<Span>,
    /// The formula for this step (Or `None` if the formula could not be parsed)
    formula: Option<Formula>,
    /// Diagnostics for this step
    diags: Vec<Diag>,
}

impl Step {
    pub fn from_str(buf: &str, database: &Database) -> Step {
        lazy_static! {
            static ref PROOF_LINE: Regex = Regex::new(
                r"(?s)([0-9a-z]+):([\?|0-9a-z]*)(?:,?([\?|0-9a-z]+))*:(\?|[0-9A-Za-z_\-\.]+)[ \t\n]+(.+)",
            ).expect("Malformed Regex");
        }
        let nset = database.name_result();
        let grammar = database.grammar_result();
        let _provable = grammar.provable_typecode();
        let mut diags = vec![];
        if let Some(caps) = PROOF_LINE.captures(&buf) {
            let label_span = caps
                .get(1)
                .expect("Regex did not return right number of captures")
                .into();
            let mut hyps = vec![];
            for idx in 2..caps.len() - 3 {
                let hyp_name = caps
                    .get(idx)
                    .expect("Regex did not return right number of captures");
                hyps.push(
                    if hyp_name.as_str() == "?" {
                        None
                    } else {
                        Some(hyp_name.into())
                    }
                );
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
            let formula_caps = caps
                .get(caps.len() - 1)
                .expect("Regex did not return right number of captures");
            let formula_span = Some(formula_caps.into());
            let formula_string = formula_caps.as_str();
            // TODO check that the formula starts with the "provable" typecode token
            let formula = match grammar.parse_string(formula_string, nset) {
                Ok(formula) => Some(formula),
                Err(diag) => {
                    diags.push(diag.into());
                    None
                }
            };
            Step {
                label_span,
                step_type: StepType::Step,
                hyps,
                label,
                formula_span,
                formula,
                diags,
            }
        } else {
            let label_span = if let Some(offset) = memchr(b':', buf.as_bytes()) {
                Span::until(offset)
            } else {
                Span::until(buf.len())
            };
            diags.push(Diag::UnparseableProofLine);
            Step {
                label_span,
                step_type: StepType::Step,
                hyps: vec![],
                label: None,
                formula_span: None,
                formula: None,
                diags,
            }
        }
    }

    #[inline]
    #[must_use]
    /// The label of this step
    pub fn label(self, buf: &str) -> &str {
        self.label_span.as_ref(buf)
    }

    #[inline]
    #[must_use]
    /// An iterator through the hypotheses of this step
    pub(crate) fn hyps(&self) -> Iter<Option<Span>> {
        self.hyps.iter()
    }

    #[inline]
    #[must_use]
    /// An iterator through the diagnostics for this step
    pub fn diags(&self) -> Iter<Diag> {
        self.diags.iter()
    }

    #[inline]
    /// Provides the range where the formula can be found
    pub(crate) fn formula_range(&self, offset: usize) -> std::ops::Range<usize> {
        self.formula_span.as_ref().unwrap().as_range(offset)
    }
}
