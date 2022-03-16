//! Representation of a Proof Step
use super::worksheet::Diag;
use super::worksheet::ProofWorksheet;
use lazy_static::lazy_static;
use memchr::memchr;
use metamath_knife::formula::Formula;
use metamath_knife::formula::Label;
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
    /// Errors and information in this worksheet
    diags: Vec<Diag>,
}

impl Step {
    fn from_str(s: &str, worksheet: ProofWorksheet) -> Self {
        lazy_static! {
            static ref PROOF_LINE: Regex = Regex::new(
                r"([0-9a-z]+):([\?|0-9a-z]*)(?:,?([\?|0-9a-z]+))*:(\?|[0-9A-Za-z_\-\.]+)[ \t\n]+(.+)",
            ).unwrap();
        }
        let nset = worksheet.db.name_result();
        let grammar = worksheet.db.grammar_result();
        let _provable = grammar.provable_typecode();
        let logic = grammar.logic_typecode();
        if let Some(caps) = PROOF_LINE.captures(s) {
            let mut diags = vec![];
            let step_name = caps.get(1).unwrap().as_str();
            let mut hyps = vec![];
            for idx in 2..caps.len() - 3 {
                let capture = caps.get(idx).unwrap();
                let hyp_name = capture.as_str();
                hyps.push(if hyp_name == "?" {
                    None
                } else if worksheet.steps_by_name.get(hyp_name).is_some() {
                    Some(hyp_name.to_string())
                } else {
                    diags.push(Diag::UnknownStepLabel(capture.range()));
                    None
                });
            }
            let label_name = caps.get(caps.len() - 2).unwrap().as_str();
            let label = nset.lookup_label(label_name.as_bytes()).map(|l| l.atom);
            let formula_string = caps.get(caps.len() - 1).unwrap().as_str();
            // TODO use re.split and also ignore tabs and line feeds
            let mut symbols = formula_string.trim().split(' ');
            let _typecode = symbols.next();
            // TODO - Check typecode is provable
            let formula = match grammar.parse_formula(
                &mut symbols.map(|t| nset.lookup_symbol(t.as_bytes()).unwrap().atom),
                &[logic],
                nset,
            ) {
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
                diags,
            }
        } else {
            let name = if let Some(offset) = memchr(b':', s.as_bytes()) {
                &s[..offset]
            } else {
                s
            };
            Step {
                name: name.into(),
                step_type: StepType::Step,
                hyps: vec![],
                label: None,
                formula: None,
                diags: vec![Diag::UnparseableProofLine],
            }
        }
    }
}
