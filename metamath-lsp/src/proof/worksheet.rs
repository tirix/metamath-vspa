use crate::proof::step::Step;
use lazy_static::lazy_static;
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range as LspRange,
    TextDocumentContentChangeEvent,
};
use metamath_knife::diag::StmtParseError;
use metamath_knife::statement::StatementAddress;
use metamath_knife::{Database, StatementRef};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::str::FromStr;
use std::sync::Arc;

use super::proof_rope::ProofDelta;
use super::ProofRope;
use crate::rope_ext::RopeExt;

// #[derive(Default)]
// /// A Position information within the proof file
// pub struct Span {
//     pub line_start: usize,
//     pub col_start: usize,
//     pub line_end: usize,
//     pub col_end: usize,
// }

// impl Span {
//     fn new(line_start: usize, col_start: usize, source: ProofRope) -> Self {
//         let len_lines = slice.len_lines();
//         Span {
//             line_start,
//             col_start,
//             line_end: line_start + len_lines,
//             col_end: slice.len_chars() - slice.line_to_char(len_lines)
//         }
//     }
// }

/// A Diagnostic
pub enum Diag {
    UnknownStepLabel(Range<usize>),
    UnknownTheoremLabel(Range<usize>),
    UnparseableFirstLine,
    UnparseableProofLine,
    DatabaseDiagnostic(StmtParseError),
    NotProvableStep,
    NoFormula,
    UnknownToken,
}

impl From<StmtParseError> for Diag {
    fn from(d: StmtParseError) -> Self {
        Self::DatabaseDiagnostic(d)
    }
}

impl Diag {
    fn message(&self) -> String {
        match self {
            Diag::UnknownStepLabel(_) => "Unknown step label".to_string(),
            Diag::UnknownTheoremLabel(_) => "Unknown theorem".to_string(),
            Diag::UnparseableFirstLine => "Could not parse first line".to_string(),
            Diag::UnparseableProofLine => "Could not parse proof line".to_string(),
            Diag::NotProvableStep => {
                "Step formula does not start with the provable typecode".to_string()
            }
            Diag::NoFormula => "No step formula found".to_string(),
            Diag::UnknownToken => "Unknown math token".to_string(),
            Diag::DatabaseDiagnostic(diag) => diag.label().to_string(),
        }
    }

    fn severity(&self) -> Option<DiagnosticSeverity> {
        Some(DiagnosticSeverity::ERROR)
    }

    fn get_range(&self, step_range: Range<usize>) -> Range<usize> {
        match self {
            Diag::UnknownStepLabel(range) | Diag::UnknownTheoremLabel(range) => Range {
                start: step_range.start + range.start,
                end: step_range.start + range.end,
            },
            Diag::UnparseableFirstLine
            | Diag::UnparseableProofLine
            | Diag::NotProvableStep
            | Diag::NoFormula => step_range,
            Diag::UnknownToken => step_range,
            Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementTooShort(span, _))
            | Diag::DatabaseDiagnostic(StmtParseError::UnknownToken(span))
            | Diag::DatabaseDiagnostic(StmtParseError::UnparseableStatement(span)) => Range {
                start: step_range.start + span.start as usize,
                end: step_range.start + span.end as usize,
            },
            Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementNoTypeCode) => step_range,
            Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementWrongTypeCode(_)) => step_range,
        }
    }
}

/// This structure is used to display a Metamath proof in the form of an MMP file:
/// A list of steps with the theorems and hypotheses used to derive each.
#[derive(Default, Clone)]
pub struct ProofWorksheet {
    /// The database used to build this worksheet
    pub(crate) db: Database,
    /// The proof rope holds the textual information, as well as the information about beginning/end of steps and lines
    pub(crate) source: ProofRope,
    /// The statement which is being proven
    sadd: Option<StatementAddress>,
    /// A position in the database. Only statements before this one are allowed in a proof.
    loc_after: Option<StatementAddress>,
    /// All the steps in this proof, referenced by their proof label (usually these are actually numbers, but any valid metamath label is allowed)
    pub(crate) steps_by_name: Arc<HashMap<String, Step>>,
    /// Diagnostics
    pub(crate) diags: Arc<Vec<(String, Diag)>>,
}

impl ProofWorksheet {
    pub fn from_reader<T: std::io::Read>(file: T, db: &Database) -> Result<Self, std::io::Error> {
        Ok(Self::new(db, &ProofRope::from_reader(file)?))
    }

    pub fn from_string(text: &str, db: &Database) -> Result<Self, std::string::ParseError> {
        let source = ProofRope::from_str(text)?;
        Ok(Self::new(db, &source))
    }

    pub fn byte_to_lsp_position(&self, byte_idx: usize) -> Position {
        self.source.byte_to_lsp_position(byte_idx)
    }

    pub fn lsp_position_to_byte(&self, position: Position) -> usize {
        self.source.lsp_position_to_byte(position)
    }

    pub fn line(&self, line_idx: u32) -> Cow<str> {
        self.source.line(line_idx)
    }

    /// Apply the changes from the provided LSP event,
    pub fn apply_changes(&mut self, changes: Vec<TextDocumentContentChangeEvent>) {
        for change in changes.iter() {
            if let Ok(delta) = ProofDelta::from_lsp_change(&self.source, change) {
                self.source = self.source.apply(delta);
            }
        }
    }

    /// Apply the changes from the provided `ProofDelta`,
    /// and update the corresponding steps
    fn apply_delta(&mut self, delta: ProofDelta) {
        // for _deleted_range in delta.deletions() {
        // // TODO here: handle deletions for self.source.
        // }
        // for _inserted_rope in delta.insertions() {
        //     // TODO here: handle insertions for self.source.
        // }
        self.source = self.source.apply(delta);
    }

    /// Creates a new proof worksheet with the given source text
    fn new(db: &Database, source: &ProofRope) -> Self {
        let mut worksheet = ProofWorksheet {
            source: source.clone(),
            db: db.clone(),
            ..Default::default()
        };
        let mut diags = vec![];
        let mut steps_by_name = HashMap::new();
        let mut steps_iter = source.steps_iter(..);
        if steps_iter.next().is_none() || worksheet.update_first_line().is_none() {
            diags.push(("".to_string(), Diag::UnparseableFirstLine));
        }

        let mut steps_names = vec![];
        for step_string in steps_iter {
            log::info!("Found step: {}", step_string);
            let (step, step_diags) = Step::from_str(&step_string, &db);
            for (hyp_name, range) in step.hyps() {
                if let Some(name) = hyp_name {
                    if !steps_names.contains(name) {
                        diags.push((
                            step.name().to_string(),
                            Diag::UnknownStepLabel(range.clone()),
                        ));
                    }
                    steps_names.push(name.to_string());
                }
            }
            for diag in step_diags {
                diags.push((step.name().to_string(), diag));
            }
            steps_by_name.insert(step.name().to_string(), step);
        }
        worksheet.diags = Arc::new(diags);
        worksheet.steps_by_name = Arc::new(steps_by_name);
        worksheet
    }

    pub fn diagnostics(&self) -> Vec<LspDiagnostic> {
        let mut diagnostics = vec![];
        for (step_name, diag) in self.diags.iter() {
            if let Some(_step) = self.steps_by_name.get(step_name) {
                let range = diag.get_range(Range { start: 0, end: 100 });
                diagnostics.push(LspDiagnostic {
                    range: LspRange {
                        start: self.byte_to_lsp_position(range.start),
                        end: self.byte_to_lsp_position(range.end),
                    },
                    severity: diag.severity(),
                    code: None,
                    code_description: None,
                    source: None,
                    message: diag.message(),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }
        }
        diagnostics
    }

    // First line has changed, update theorem name, loc_after
    fn update_first_line(&mut self) -> Option<()> {
        lazy_static! {
            static ref FIRST_LINE: Regex = Regex::new(
                r"^\$\( <MM> <PROOF_ASST> THEOREM=([0-9A-Za-z_\-\.]+)  LOC_AFTER=(\?|[0-9A-Za-z_\-\.]+)",
            ).unwrap();
        }
        let first_line = &self.source.line(0);
        log::info!("Found first line: {}", first_line);
        FIRST_LINE.captures(first_line).map(|caps| {
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
        })
    }

    // /// Update the internation representation of the proof, for the lines changed.
    // pub fn update(&mut self, mut line_nums: Range<usize>) {
    //     if line_nums.contains(&0) {
    //         self.update_first_line();
    //     }

    //     // Adjust the first line to the line with label
    //     while line_nums.start > 0 && is_followup_line(&self.source, line_nums.start) {
    //         line_nums.start -= 1;
    //     }
    //     if line_nums.start == 0 {
    //         line_nums.start = 1;
    //         while is_followup_line(&self.source, line_nums.start) {
    //             line_nums.start += 1;
    //         }
    //     }

    //     // Adjust the last line to include followup lines
    //     while line_nums.end < self.source.len_lines() - 1
    //         && is_followup_line(&self.source, line_nums.end)
    //     {
    //         line_nums.end += 1;
    //     }

    //     // Remove the steps at the previous lines, and the associated errors
    //     // TODO

    //     // Actually attempt to parse the new lines
    //     let nset = self.db.name_result().clone();
    //     let grammar = self.db.grammar_result().clone();
    //     let mut current_line = vec![];
    //     let mut step_lines = Range::default();
    //     for line_num in line_nums {
    //         if is_followup_line(&self.source, line_num) || current_line.is_empty() {
    //             // This is a follow-up line, just concatenate it.
    //         } else if current_line[0] == b'*' {
    //             // Comment, skip.
    //             info!("Skipping comment line {}", as_str(&current_line));
    //             current_line = vec![]; // TODO find how to empty
    //             step_lines = line_num..line_num;
    //         } else {
    //             let step = Step::from_str(as_str(&current_line))?;
    //             self.steps_by_line
    //                 .insert(step.lines.start, step.name);
    //             self.steps_by_name.insert(step_name.to_string(), step);
    //             current_line = vec![]; // TODO find how to empty
    //             step_lines = line_num..line_num;
    //         }
    //         // TODO optimize!
    //         write!(current_line, "{} ", self.source.line(line_num).to_string()).ok();
    //         step_lines.end = line_num + 1;
    //     }
    // }
}
