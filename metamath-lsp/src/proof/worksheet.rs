use crate::proof::step::Step;
use lsp_types::{Position, TextDocumentContentChangeEvent};
use metamath_knife::diag::Diagnostic;
use metamath_knife::statement::StatementAddress;
use metamath_knife::Database;
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
    UnparseableFirstLine,
    UnparseableProofLine,
    DatabaseDiagnostic(Diagnostic),
}

impl From<Diagnostic> for Diag {
    fn from(d: Diagnostic) -> Self {
        Self::DatabaseDiagnostic(d)
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
        // TODO here: initialize steps
        ProofWorksheet {
            source: source.clone(),
            db: db.clone(),
            ..Default::default()
        }
    }

    // // First line has changed, update theorem name, loc_after
    // fn update_first_line(&mut self) {
    //     lazy_static! {
    //         static ref FIRST_LINE: Regex = Regex::new(
    //             r"^\$\( <MM> <PROOF_ASST> THEOREM=([0-9A-Za-z_\-\.]+)  LOC_AFTER=(\?|[0-9A-Za-z_\-\.]+)$",
    //         ).unwrap();
    //     }
    //     match FIRST_LINE.captures(self.source.line(0).as_str().unwrap()) {
    //         Some(caps) => {
    //             let statement_name = caps.get(1).unwrap().as_str();
    //             let loc_after_name = caps.get(2).unwrap().as_str();
    //             self.sadd = self
    //                 .db
    //                 .statement(statement_name.as_bytes())
    //                 .map(StatementRef::address);
    //             self.loc_after = self
    //                 .db
    //                 .statement(loc_after_name.as_bytes())
    //                 .map(StatementRef::address);
    //         }
    //         None => {
    //             error!("Could not parse first line!");
    //             self.diag(0, Diag::UnparseableFirstLine(Span::new(0, 0, self.source.line(0))));
    //         }
    //     }
    // }

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
