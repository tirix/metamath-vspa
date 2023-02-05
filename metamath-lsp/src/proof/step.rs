//! Representation of a Proof Step
use std::slice::Iter;

use super::worksheet::{Diag, Span, StepIdx};
use super::ProofWorksheet;
use lazy_static::lazy_static;
use memchr::memchr;
use metamath_knife::formula::{Label, Substitutions};
use metamath_knife::Database;
use metamath_knife::Formula;
use regex::Regex;

/// The type of the step: hypothesis, normal proof step of QED step.
#[derive(Clone, Debug)]
enum StepType {
    Hyp,
    Step,
    Qed,
    Error,
}

/// A Proof Step.
#[derive(Clone, Debug)]
pub struct Step {
    /// Label of this step
    name_span: Span,
    /// Type of this step
    step_type: StepType,
    /// The span where the hypothesis list can be found
    hyps_span: Span,
    /// Hypotheses
    hyps: Vec<Span>,
    /// span of the label
    label_span: Span,
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
                r"(?s)(h?)([0-9a-z]+):((?:\?|[0-9a-z]*)(?:,(?:\?|[0-9a-z]+))*):(\?|[0-9A-Za-z_\-\.]+)[ \t\n]+(.+)",
            ).expect("Malformed Regex");
        }
        let nset = database.name_result();
        let grammar = database.grammar_result();
        let _provable = grammar.provable_typecode();
        let mut diags = vec![];
        if let Some(caps) = PROOF_LINE.captures(buf) {
            let name_span = caps
                .get(2)
                .expect("Regex did not return right number of captures")
                .into();
            let capture = caps
                .get(3)
                .expect("Regex did not return right number of captures");
            let hyps_span: Span = capture.into();
            let mut hyps = vec![];
            let mut pos = hyps_span.as_range(0).start;
            for hyp in capture.as_str().split(',') {
                if !hyp.is_empty() {
                    hyps.push(Span::new(pos, hyp.len()));
                }
                pos += 1 + hyp.len();
            }
            let capture = caps
                .get(4)
                .expect("Regex did not return right number of captures");
            let label_span = capture.into();
            let label = nset
                .lookup_label(capture.as_str().as_bytes())
                .map(|l| l.atom);
            if label.is_none() {
                diags.push(Diag::UnknownTheoremLabel(capture.range()));
            }
            let step_type = match capture.as_str() {
                "" => StepType::Error,
                "qed" => StepType::Qed,
                _ => {
                    if caps
                        .get(1)
                        .expect("Regex did not return right number of captures")
                        .as_str()
                        == "h"
                    {
                        StepType::Hyp
                    } else {
                        StepType::Step
                    }
                }
            };
            let formula_caps = caps
                .get(5)
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
                name_span,
                step_type,
                hyps_span,
                hyps,
                label_span,
                label,
                formula_span,
                formula,
                diags,
            }
        } else {
            let name_span = if let Some(offset) = memchr(b':', buf.as_bytes()) {
                Span::until(offset)
            } else {
                Span::until(buf.len())
            };
            diags.push(Diag::UnparseableProofLine);
            Step {
                name_span,
                step_type: StepType::Error,
                hyps_span: Span::default(),
                hyps: vec![],
                label_span: Span::default(),
                label: None,
                formula_span: None,
                formula: None,
                diags,
            }
        }
    }

    #[inline]
    /// The label of this step
    pub(crate) fn name_span(&self) -> &Span {
        &self.name_span
    }

    #[inline]
    /// An iterator through the hypotheses of this step
    pub(crate) fn hyps(&self) -> Iter<Span> {
        self.hyps.iter()
    }

    /// The span corresponding to the hypothesis reference
    pub(crate) fn hyp_ref_span(&self, hyp_idx: usize) -> &Span {
        &self.hyps[hyp_idx]
    }

    pub(crate) fn hyps_span(&self) -> &Span {
        &self.hyps_span
    }

    #[inline]
    #[must_use]
    /// The label of this step
    pub fn label<'a>(&self, buf: &'a str) -> &'a str {
        self.label_span.as_ref(buf)
    }

    #[inline]
    #[must_use]
    /// This step's formula
    pub fn formula(&self) -> Option<&Formula> {
        self.formula.as_ref()
    }

    #[inline]
    /// An iterator through the diagnostics for this step
    pub fn diags(&self) -> Iter<Diag> {
        self.diags.iter()
    }

    /// Adds a new diagnostic
    pub(crate) fn push_diag(&mut self, diag: Diag) {
        self.diags.push(diag);
    }

    #[inline]
    /// Provides the range where the formula can be found
    pub(crate) fn formula_range(&self, offset: usize) -> std::ops::Range<usize> {
        self.formula_span.as_ref().unwrap().as_range(offset)
    }

    /// Checks that this step can be derived
    pub fn validate(&self, step_idx: StepIdx, worksheet: &ProofWorksheet) -> Result<(), Diag> {
        match self.step_type {
            StepType::Hyp => {
                // Hypothesis step: validate that it matches the statement
                let label_name = worksheet.step_label(step_idx);
                if let Some(sref) = worksheet.db.statement(label_name) {
                    if self.formula.as_ref() != worksheet.db.stmt_parse_result().get_formula(&sref)
                    {
                        return Err(Diag::HypothesisDoesNotMatch);
                    }
                }
            }
            StepType::Qed => {
                // QED step: validate that it matches the statement
                self.check_unification(step_idx, worksheet)?;
                if let Some(sadd) = worksheet.sadd {
                    if self.formula.as_ref()
                        != worksheet
                            .db
                            .stmt_parse_result()
                            .get_formula(&worksheet.db.statement_by_address(sadd))
                    {
                        return Err(Diag::HypothesisDoesNotMatch);
                    }
                }
            }
            StepType::Step => {
                self.check_unification(step_idx, worksheet)?;
            }
            _ => (),
        };
        Ok(())
    }

    fn check_unification(&self, step_idx: StepIdx, worksheet: &ProofWorksheet) -> Result<(), Diag> {
        let formula = worksheet.step_stmt_formula(step_idx)?;
        let label_name = worksheet.step_label(step_idx);
        let frame = worksheet.db.scope_result().get(label_name).unwrap();
        let essentials: Vec<_> = frame.as_ref(&worksheet.db).essentials().collect();
        if essentials.len() != self.hyps.len() {
            return Err(Diag::WrongHypCount {
                expected: essentials.len(),
                actual: self.hyps.len(),
            });
        }
        let mut substitutions = Substitutions::new();
        self.formula
            .as_ref()
            .ok_or(Diag::NoFormula)?
            .unify(formula, &mut substitutions)?;
        for (hyp_idx, (_, formula)) in essentials.into_iter().enumerate() {
            let step_name = worksheet.hyp_name(step_idx, hyp_idx);
            if let Some(&hyp_step_idx) = worksheet.steps_by_name.get(step_name) {
                if let Some(hyp_formula) = worksheet.step_formula(hyp_step_idx) {
                    let mut hyp_subst = Substitutions::new();
                    hyp_formula
                        .unify(formula, &mut hyp_subst)
                        .map_err(|_| Diag::UnificationFailedForHyp(hyp_idx))?;
                    Self::check_and_extend(&mut substitutions, &hyp_subst, hyp_idx)?;
                }
            } else {
                return Err(Diag::UnknownStepName(self.hyps[hyp_idx].as_range(0)));
            }
        }
        Ok(())
    }

    // TODO - move this check that those substitutions are compatible to metamath_knife!!
    fn check_and_extend(
        s1: &mut Substitutions,
        s2: &Substitutions,
        hyp_idx: usize,
    ) -> Result<(), Diag> {
        for (&label, f1) in s1.into_iter() {
            if let Some(f2) = s2.get(label) {
                if f1 != f2 {
                    return Err(Diag::UnificationFailedForHyp(hyp_idx));
                }
            }
        }
        s1.extend(s2);
        Ok(())
    }
}
