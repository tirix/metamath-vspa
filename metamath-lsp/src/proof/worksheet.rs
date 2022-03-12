use crate::proof::step::Step;
use lazy_static::lazy_static;
use log::*;
use metamath_knife::diag::Diagnostic;
use metamath_knife::statement::as_str;
use metamath_knife::statement::StatementAddress;
use metamath_knife::statement::StatementRef;
use metamath_knife::Database;
use regex::Regex;
use std::collections::HashMap;
use std::io::Write;
use std::ops::Range;
use std::sync::Arc;

use super::ProofRope;

#[derive(Default)]
/// A Position information within the proof file
pub struct Span {
    pub line_start: usize,
    pub col_start: usize,
    pub line_end: usize,
    pub col_end: usize,
}

impl Span {
    fn new(line_start: usize, col_start: usize, source: ProofRope) -> Self {
        let len_lines = slice.len_lines();
        Span {
            line_start,
            col_start,
            line_end: line_start + len_lines,
            col_end: slice.len_chars() - slice.line_to_char(len_lines) 
        }
    }
}

/// A Diagnostic
pub enum Diag {
    UnparseableFirstLine(Span),
    UnparseableProofLine(Span),
    DatabaseDiagnostic(Diagnostic),
}

#[derive(Default, Clone)]
pub struct ProofWorksheet {
    source: ProofRope,
    db: Database,
    diags: Arc<HashMap<usize, Vec<Diag>>>,
    sadd: Option<StatementAddress>,
    loc_after: Option<StatementAddress>,
    steps_by_name: Arc<HashMap<String, Step>>,
}

impl ProofWorksheet {
    pub fn from_reader<T: std::io::Read>(db: Database, file: T) -> Self {
        Self::new(db, &ProofRope::from_reader(file).expect("Could not open workseet file"))
    }

    /// Creates a new proof worksheet with the given source text
    fn new(db: Database, source: &ProofRope) -> Self {
        let mut worksheet = ProofWorksheet {
            source: source.clone(),
            db,
            ..Default::default()
        };
        worksheet.update(0..source.len_lines() - 1);
        worksheet
    }

    pub fn diag(&mut self, line: usize, diag: Diag) {
        self.diags.entry(line).or_insert_with(Vec::new).push(diag);
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
                error!("Could not parse first line!");
                self.diag(0, Diag::UnparseableFirstLine(Span::new(0, 0, self.source.line(0))));
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

        // Remove the steps at the previous lines, and the associated errors
        // TODO

        // Actually attempt to parse the new lines
        let nset = self.db.name_result().clone();
        let grammar = self.db.grammar_result().clone();
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
                        let formula = match grammar.parse_formula(
                            &mut symbols
                                .map(|t| nset.lookup_symbol(t.as_bytes()).unwrap().atom),
                            &[logic],
                            &nset,
                        ) {
                            Ok(formula) => {
                                info!(
                                    "Matched line {}, formula {}",
                                    line_num,
                                    formula.clone().as_ref(&self.db)
                                );
                                Some(formula)
                            },
                            Err(diag) => {
                                self.diag(line_num, Diag::DatabaseDiagnostic(diag));
                                None
                            },
                        };
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
                        error!("Could not parse line {}", as_str(&current_line));
                        self.diag(line_num, Diag::UnparseableProofLine(Span::new(0, 0, self.source.line(0))));
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
