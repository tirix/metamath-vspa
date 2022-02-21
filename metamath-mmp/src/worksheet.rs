use lazy_static::lazy_static;
use log::*;
use metamath_knife::formula::Formula;
use metamath_knife::formula::Label;
use metamath_knife::statement::as_str;
use metamath_knife::statement::StatementAddress;
use metamath_knife::statement::StatementRef;
use metamath_knife::Database;
use regex::Regex;
use ropey::Rope;
use std::collections::HashMap;
use std::io::Write;
use std::ops::Range;

/// The type of the step: hypothesis, normal proof step of QED step.
enum StepType {
    Hyp,
    Step,
    Qed,
}

struct Step {
    name: String,
    step_type: StepType,
    lines: Range<usize>,
    hyps: Vec<Option<String>>,
    label: Option<Label>,
    formula: Option<Formula>,
}

#[derive(Default)]
pub struct ProofWorksheet {
    source: Rope,
    db: Database,
    sadd: Option<StatementAddress>,
    loc_after: Option<StatementAddress>,
    steps_by_name: HashMap<String, Step>,
    steps_by_line: HashMap<usize, String>,
}

/// Checks whether the line is a follow-up of the previous line
/// A line starting with a space to tab shall simply be concatenated with the previous one
#[inline]
fn is_followup_line(source: &Rope, line_idx: usize) -> bool {
    matches!(
        source.char(source.line_to_char(line_idx)),
        ' ' | '\t' | '\n'
    )
}

impl ProofWorksheet {
    /// Creates a new proof worksheet with the given source text
    pub fn new(db: Database, source: &Rope) -> Self {
        let mut worksheet = ProofWorksheet {
            source: source.clone(),
            db,
            ..Default::default()
        };
        worksheet.update(0..source.len_lines() - 1);
        worksheet
    }

    // First line has changed, update theorem name, loc_after
    fn update_first_line(&mut self) {
        lazy_static! {
            static ref FIRST_LINE: Regex = Regex::new(
                r"^\$\( <MM> <PROOF_ASST> THEOREM=([0-9A-Za-z_\-\.]+)  LOC_AFTER=(\?|[0-9A-Za-z_\-\.]+)$",
            ).unwrap();
        }
        match FIRST_LINE.captures(self.source.line(0).as_str().unwrap()) {
            Some(caps) => {
                let statement_name = caps.get(1).unwrap().as_str();
                let loc_after_name = caps.get(2).unwrap().as_str();
                self.sadd = self
                    .db
                    .statement(statement_name.as_bytes())
                    .map(StatementRef::address);
                self.loc_after = self
                    .db
                    .statement(loc_after_name.as_bytes())
                    .map(StatementRef::address);
            }
            None => {
                // TODO - Diagnostics!
                error!("Could not parse first line!");
            }
        }
    }

    /// Update the internation representation of the proof, for the lines changed.
    pub fn update(&mut self, mut line_nums: Range<usize>) {
        lazy_static! {
            static ref PROOF_LINE: Regex = Regex::new(
                r"([0-9a-z]+):([\?|0-9a-z]*)(?:,?([\?|0-9a-z]+))*:(\?|[0-9A-Za-z_\-\.]+)[ \t\n]+(.+)",
            ).unwrap();
        }
        if line_nums.contains(&0) {
            self.update_first_line();
        }

        // Adjust the first line to the line with label
        while line_nums.start > 0 && is_followup_line(&self.source, line_nums.start) {
            line_nums.start -= 1;
        }
        if line_nums.start == 0 {
            line_nums.start = 1;
            while is_followup_line(&self.source, line_nums.start) {
                line_nums.start += 1;
            }
        }

        // Adjust the last line to include followup lines
        while line_nums.end < self.source.len_lines() - 1
            && is_followup_line(&self.source, line_nums.end)
        {
            line_nums.end += 1;
        }

        // Actually attempt to parse the new lines
        let nset = self.db.name_result();
        let grammar = self.db.grammar_result();
        let _provable = grammar.provable_typecode();
        let logic = nset.lookup_symbol(b"wff").unwrap().atom; // TODO - need metamath-knife API to get grammar's logic type.
        let mut current_line = vec![];
        let mut step_lines = Range::default();
        for line_num in line_nums {
            if is_followup_line(&self.source, line_num) || current_line.is_empty() {
                // This is a follow-up line, just concatenate it.
            } else if current_line[0] == b'*' {
                // Comment, skip.
                info!("Skipping comment line {}", as_str(&current_line));
                current_line = vec![]; // TODO find how to empty
                step_lines = line_num..line_num;
            } else {
                // TODO optimize!
                match PROOF_LINE.captures(as_str(&current_line)) {
                    Some(caps) => {
                        let step_name = caps.get(1).unwrap().as_str();
                        let mut hyps = vec![];
                        for idx in 2..caps.len() - 3 {
                            let hyp_name = caps.get(idx).unwrap().as_str();
                            let hyp = self
                                .steps_by_name
                                .get(hyp_name)
                                .map(|_| hyp_name.to_string());
                            hyps.push(hyp);
                        }
                        let label_name = caps.get(caps.len() - 2).unwrap().as_str();
                        let label = nset.lookup_label(label_name.as_bytes()).map(|l| l.atom);
                        let formula_string = caps.get(caps.len() - 1).unwrap().as_str();
                        // TODO use re.split and also ignore tabs and line feeds
                        let mut symbols = formula_string.trim().split(' ');
                        let _typecode = symbols.next();
                        // TODO - Check typecode is provable
                        let formula = grammar
                            .parse_formula(
                                &mut symbols
                                    .map(|t| nset.lookup_symbol(t.as_bytes()).unwrap().atom),
                                &[logic],
                                nset,
                            )
                            .ok();
                        info!(
                            "Matched line {}, formula {}",
                            line_num,
                            formula.clone().unwrap().as_ref(&self.db)
                        );
                        let step = Step {
                            name: step_name.to_string(),
                            step_type: StepType::Step,
                            hyps,
                            label,
                            formula,
                            lines: step_lines,
                        };
                        self.steps_by_line
                            .insert(step.lines.start, step_name.to_string());
                        self.steps_by_name.insert(step_name.to_string(), step);
                    }
                    None => {
                        // TODO - Diagnostics!
                        error!("Could not parse line {}", as_str(&current_line));
                    }
                }
                current_line = vec![]; // TODO find how to empty
                step_lines = line_num..line_num;
            }
            // TODO optimize!
            write!(current_line, "{} ", self.source.line(line_num).to_string()).ok();
            step_lines.end = line_num + 1;
        }
    }
}
