use crate::proof::step::Step;
use lazy_static::lazy_static;
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range as LspRange,
    TextDocumentContentChangeEvent,
};
use metamath_knife::diag::StmtParseError;
use metamath_knife::formula::UnificationError;
use metamath_knife::statement::{StatementAddress, TokenPtr};
use metamath_knife::{Database, Formula, StatementRef};
use regex::{Match, Regex};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::{Index, Range};

/// A Diagnostic
#[derive(Clone, Debug)]
pub enum Diag {
    UnknownStepName(Range<usize>),
    UnknownTheoremLabel(Range<usize>),
    UnparseableFirstLine,
    UnparseableProofLine,
    DatabaseDiagnostic(StmtParseError),
    NotProvableStep,
    NoFormula,
    UnknownToken,
    HypothesisDoesNotMatch,
    ProofDoesNotMatch,
    WrongHypCount { expected: usize, actual: usize },
    UnificationFailed,
    UnificationFailedForHyp(usize),
}

impl From<StmtParseError> for Diag {
    fn from(d: StmtParseError) -> Self {
        Self::DatabaseDiagnostic(d)
    }
}

impl From<UnificationError> for Diag {
    fn from(_: UnificationError) -> Self {
        Self::UnificationFailed
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Span(Range<usize>);

impl From<Match<'_>> for Span {
    fn from(m: Match) -> Self {
        Self(m.range())
    }
}

impl Span {
    #[inline]
    #[must_use]
    pub fn new(start: usize, len: usize) -> Self {
        Self(start..start + len)
    }

    #[inline]
    #[must_use]
    pub fn until(end: usize) -> Self {
        Self(0..end)
    }

    #[inline]
    #[must_use]
    pub fn as_ref<'a>(&self, buf: &'a str) -> &'a str {
        &buf[self.0.clone()]
    }

    #[inline]
    #[must_use]
    pub fn as_range(&self, offset: usize) -> Range<usize> {
        Range {
            start: offset + self.0.start,
            end: offset + self.0.end,
        }
    }
}

impl From<&Span> for Range<usize> {
    fn from(span: &Span) -> Self {
        span.0.clone()
    }
}

impl Diag {
    fn message(&self) -> String {
        match self {
            Diag::UnknownStepName(_) => "Unknown step name".to_string(),
            Diag::UnknownTheoremLabel(_) => "Unknown theorem".to_string(),
            Diag::UnparseableFirstLine => "Could not parse first line".to_string(),
            Diag::UnparseableProofLine => "Could not parse proof line".to_string(),
            Diag::NotProvableStep => {
                "Step formula does not start with the provable typecode".to_string()
            }
            Diag::NoFormula => "No step formula found".to_string(),
            Diag::UnknownToken => "Unknown math token".to_string(),
            Diag::DatabaseDiagnostic(diag) => diag.label().to_string(),
            Diag::HypothesisDoesNotMatch => {
                "Hypothesis formula does not match database".to_string()
            }
            Diag::ProofDoesNotMatch => "Proof formula does not match database".to_string(),
            Diag::WrongHypCount { expected, actual } => {
                format!("Wrong hypotheses count: expected {expected}, got {actual}")
            }
            Diag::UnificationFailed => "Unification failed".to_string(),
            Diag::UnificationFailedForHyp(_) => "Unification failed for hypothesis".to_string(),
        }
    }

    fn severity(&self) -> Option<DiagnosticSeverity> {
        Some(DiagnosticSeverity::ERROR)
    }

    fn get_range(&self, step_info: &StepInfo) -> Range<usize> {
        let step_span = step_info.byte_idx..step_info.byte_idx + step_info.source.len();
        match self {
            Diag::UnknownStepName(range) | Diag::UnknownTheoremLabel(range) => Range {
                start: step_info.byte_idx + range.start,
                end: step_info.byte_idx + range.end,
            },
            Diag::UnparseableFirstLine
            | Diag::UnparseableProofLine
            | Diag::NotProvableStep
            | Diag::NoFormula => step_span,
            Diag::UnknownToken => step_span,
            Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementTooShort(span, _))
            | Diag::DatabaseDiagnostic(StmtParseError::UnknownToken(span))
            | Diag::DatabaseDiagnostic(StmtParseError::UnparseableStatement(span)) => Range {
                start: step_info.step.formula_range(step_info.byte_idx).start + span.start as usize,
                end: step_info.step.formula_range(step_info.byte_idx).start + span.end as usize,
            },
            Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementNoTypeCode)
            | Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementWrongTypeCode(_))
            | Diag::ProofDoesNotMatch
            | Diag::HypothesisDoesNotMatch
            | Diag::UnificationFailed => step_info.step.formula_range(step_info.byte_idx),
            Diag::WrongHypCount { .. } => step_info.step.hyps_span().as_range(step_info.byte_idx),
            Diag::UnificationFailedForHyp(hyp_idx) => step_info
                .step
                .hyp_ref_span(*hyp_idx)
                .as_range(step_info.byte_idx),
        }
    }
}

/// Identifies a proof step in a worksheet.
/// This is internal to the [ProofWorksheet]
pub(crate) type StepIdx = usize;

/// Information relative to a step
/// The "source" string of each step is cloned to be stored in the step info.
#[derive(Debug)]
pub(crate) struct StepInfo {
    pub(crate) byte_idx: usize,
    pub(crate) line_idx: usize,
    pub(crate) source: String,
    pub(crate) step: Step,
}

impl StepInfo {
    fn last_byte_idx(&self) -> usize {
        self.byte_idx + self.source.len()
    }
}

/// If there is any space character at the beginning of a line,
/// it is a follow-up of the previous line, and belongs to the same step.
#[inline]
fn is_followup_char(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n' || c == b'*' || c == b'$'
}

/// Find the first step start, if any
fn find_step_start(s: &[u8]) -> Option<usize> {
    let mut step_start_idx = 0;
    loop {
        if step_start_idx >= s.len() {
            None?;
        }
        step_start_idx += memchr::memchr(b'\n', &s[step_start_idx..])? + 1;
        if step_start_idx >= s.len() {
            None?;
        }
        if !is_followup_char(s[step_start_idx]) {
            break;
        }
    }
    Some(step_start_idx)
}

/// Find the end of the actual step, without any trailing comments/proof
fn find_step_end(s: &[u8]) -> Option<usize> {
    let mut step_end_idx = 0;
    loop {
        if step_end_idx >= s.len() {
            None?;
        }
        step_end_idx += memchr::memchr(b'\n', &s[step_end_idx..])? + 1;
        if step_end_idx >= s.len() {
            None?;
        }
        if s[step_end_idx] == b'*' || s[step_end_idx] == b'$' {
            break;
        }
    }
    Some(step_end_idx)
}

/// Count the number of lines in a given string
fn line_count(s: &str) -> usize {
    bytecount::count(s.as_bytes(), b'\n')
}

/// This structure is used to display a Metamath proof in the form of an MMP file:
/// A list of steps with the theorems and hypotheses used to derive each.
#[derive(Debug, Default)]
pub struct ProofWorksheet {
    /// The database used to build this worksheet
    pub(crate) db: Database,
    /// The statement which is being proven
    pub(crate) sadd: Option<StatementAddress>,
    /// A position in the database. Only statements before this one are allowed in a proof.
    loc_after: Option<StatementAddress>,
    /// Top line and first comment
    top: String,
    /// All the steps in this proof, in the order they appear
    pub(crate) steps: Vec<StepInfo>,
    /// The indices of the steps in this proof, referenced by their proof label (usually these are actually numbers, but any valid metamath label is allowed)
    pub(crate) steps_by_name: HashMap<String, StepIdx>,
}

impl Index<&str> for ProofWorksheet {
    type Output = Step;

    fn index(&self, index: &str) -> &Self::Output {
        &self.step_info(self.steps_by_name[index]).step
    }
}

impl ProofWorksheet {
    pub fn from_reader<T: std::io::Read>(
        mut file: T,
        db: &Database,
    ) -> Result<Self, std::io::Error> {
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        Self::from_string(buffer, db)
    }

    pub fn from_string(text: String, db: &Database) -> Result<Self, std::io::Error> {
        let mut worksheet = ProofWorksheet {
            db: db.clone(),
            ..ProofWorksheet::default()
        };
        worksheet.apply_change(&TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text,
        });
        Ok(worksheet)
    }

    fn step_info(&self, index: StepIdx) -> &StepInfo {
        &self.steps[index]
    }

    pub(crate) fn step_at_line(&self, line_idx: usize) -> Option<&StepInfo> {
        let step_idx = self
            .steps
            .binary_search_by(|s| s.line_idx.cmp(&line_idx))
            .map_or_else(|step_idx| step_idx, |step_idx| step_idx + 1);
        if step_idx > 0 {
            Some(self.step_info(step_idx - 1))
        } else {
            None
        }
    }

    pub fn byte_to_lsp_position(&self, byte_idx: usize) -> Position {
        let step_idx = self
            .steps
            .binary_search_by(|s| s.byte_idx.cmp(&byte_idx))
            .map_or_else(|step_idx| step_idx, |step_idx| step_idx + 1);
        let (start_byte_idx, start_line_idx, source) = if step_idx > 0 {
            let step_info = &self.step_info(step_idx - 1);
            (step_info.byte_idx, step_info.line_idx, &step_info.source)
        } else {
            (0, 0, &self.top)
        };
        let line_idx = line_count(&source[0..byte_idx - start_byte_idx]);
        let line_start_idx =
            memchr::memrchr(b'\n', source[0..byte_idx - start_byte_idx].as_bytes()).unwrap_or(0);
        Position {
            line: (start_line_idx + line_idx) as u32,
            character: (byte_idx - line_start_idx - start_byte_idx) as u32,
        }
    }

    pub fn lsp_position_to_byte(&self, position: Position) -> usize {
        let step_idx = self
            .steps
            .binary_search_by(|s| s.line_idx.cmp(&(position.line as usize)))
            .map_or_else(|step_idx| step_idx, |step_idx| step_idx + 1);
        let (start_byte_idx, mut line_idx, source) = if step_idx > 0 {
            let step_info = &self.step_info(step_idx - 1);
            (step_info.byte_idx, step_info.line_idx, &step_info.source)
        } else {
            (0, 0, &self.top)
        };
        let mut byte_idx = 0;
        while line_idx < position.line as usize {
            byte_idx += memchr::memchr(b'\n', source[byte_idx..].as_bytes()).unwrap_or(0) + 1;
            line_idx += 1;
        }
        start_byte_idx + byte_idx + position.character as usize
    }

    /// Returns the (zero-based) line
    pub fn line(&self, target_line_idx: u32) -> Cow<str> {
        let step_idx = self
            .steps
            .binary_search_by(|s| s.line_idx.cmp(&(target_line_idx as usize)))
            .map_or_else(|step_idx| step_idx, |step_idx| step_idx + 1);
        let (mut line_idx, source) = if step_idx > 0 {
            let step_info = &self.step_info(step_idx - 1);
            (step_info.line_idx, &step_info.source)
        } else {
            (0, &self.top)
        };
        let mut byte_idx = 0;
        while line_idx < target_line_idx as usize {
            byte_idx += memchr::memchr(b'\n', source[byte_idx..].as_bytes()).unwrap_or(0) + 1;
            line_idx += 1;
        }
        let end_byte_idx =
            byte_idx + memchr::memchr(b'\n', source[byte_idx..].as_bytes()).unwrap_or(0);
        source[byte_idx..end_byte_idx].into()
    }

    /// Returns the step containing the given position,
    /// As well as the index of that position relative to the span
    #[inline]
    fn step_at(&self, position: Position) -> (Option<StepIdx>, usize) {
        let step_idx = self
            .steps
            .binary_search_by(|s| s.line_idx.cmp(&(position.line as usize)))
            .map_or_else(|step_idx| step_idx, |step_idx| step_idx + 1);
        let (step_idx, mut line_idx, source) = if step_idx > 0 {
            let step_info = &self.step_info(step_idx - 1);
            (Some(step_idx - 1), step_info.line_idx, &step_info.source)
        } else {
            (None, 0, &self.top)
        };
        let mut byte_idx = 0;
        while line_idx < position.line as usize && byte_idx < source.len() {
            byte_idx += memchr::memchr(b'\n', source[byte_idx..].as_bytes()).unwrap_or(0) + 1;
            line_idx += 1;
        }
        (step_idx, byte_idx + position.character as usize)
    }

    /// Apply the changes from the provided LSP event.
    pub fn apply_change(&mut self, change: &TextDocumentContentChangeEvent) {
        // Get changed range
        let range = change.range.unwrap_or(LspRange {
            start: self.byte_to_lsp_position(0),
            end: self
                .byte_to_lsp_position(self.steps.iter().last().map_or(0, StepInfo::last_byte_idx)),
        });

        // Find out the first step and last step impacted.
        let (mut first_step_idx, mut first_byte_idx) = self.step_at(range.start);
        let (last_step_idx, last_byte_idx) = self.step_at(range.end);
        // If the change text starts with a newline and is at the start of a step, it will be counted with the previous step
        if first_byte_idx == 0
            && change.text.starts_with('\n')
            && first_step_idx.map_or(false, |i| i > 0)
        {
            first_step_idx = Some(first_step_idx.unwrap() - 1);
            first_byte_idx = self.steps[first_step_idx.unwrap()].source.len();
        }
        let first_source = first_step_idx.map_or(&self.top, |i| &self.step_info(i).source);
        let last_source = last_step_idx.map_or(&self.top, |i| &self.step_info(i).source);

        // So we can recover the full new text
        let mut new_text = String::new();
        new_text.push_str(Span(0..first_byte_idx).as_ref(first_source));
        new_text.push_str(&change.text);
        new_text.push_str(Span(last_byte_idx..last_source.len()).as_ref(last_source));
        let mut new_text = new_text.as_str();

        // First handle the "top" part, until the first step.
        if first_step_idx.is_none() {
            let eos = find_step_start(new_text.as_bytes()).unwrap_or(new_text.len());
            self.top = new_text[0..eos].to_string();
            self.update_first_line();
            new_text = &new_text[eos..];
        }

        // Then handle the remaining text as steps
        let mut add_steps = vec![];
        let start_step_idx = first_step_idx.unwrap_or(0);
        let start_byte_idx =
            first_step_idx.map_or_else(|| self.top.len(), |i| self.steps[i].byte_idx);
        let start_line_idx =
            first_step_idx.map_or_else(|| line_count(&self.top), |i| self.steps[i].line_idx);
        let mut byte_idx = start_byte_idx;
        let mut line_idx = start_line_idx;
        let _step_start_idx = 0;
        while !new_text.is_empty() {
            let step_len = find_step_start(new_text.as_bytes()).unwrap_or(new_text.len());
            let step_line_count = line_count(&new_text[..step_len]);
            let step_end = find_step_end(new_text[..step_len].as_bytes()).unwrap_or(step_len);
            let source = new_text[..step_len].to_owned();
            let step = Step::from_str(&new_text[..step_end], &self.db);
            add_steps.push(StepInfo {
                byte_idx,
                line_idx,
                source,
                step,
            });
            byte_idx += step_len;
            line_idx += step_line_count;
            new_text = &new_text[step_len..];
        }

        // Then, information relative to all subsequent steps need to be shifted
        if let Some(end_step_idx) = last_step_idx {
            let add_byte_idx = byte_idx - start_byte_idx;
            let add_line_idx = line_idx - start_line_idx;
            let sub_byte_idx =
                self.steps[end_step_idx].last_byte_idx() - self.steps[start_step_idx].byte_idx;
            let sub_line_idx: usize = self.steps[end_step_idx].line_idx
                + line_count(&self.steps[end_step_idx].source)
                - self.steps[start_step_idx].line_idx;
            for step_idx in last_step_idx.unwrap_or(0) + 1..self.steps.len() {
                self.steps[step_idx].byte_idx -= sub_byte_idx;
                self.steps[step_idx].byte_idx += add_byte_idx;
                self.steps[step_idx].line_idx -= sub_line_idx;
                self.steps[step_idx].line_idx += add_line_idx;
            }
        }

        // Remove the old steps from the reference table, and add the new ones
        let old_step_range = first_step_idx.unwrap_or(0)..last_step_idx.map(|i| i + 1).unwrap_or(0);
        for step_idx in old_step_range.clone() {
            let step_name = self.step_name(step_idx).to_owned();
            self.steps_by_name.remove(&step_name);
        }

        // Finally, we can replace the new steps into our reference
        self.steps.splice(old_step_range, add_steps);

        // And we update step names and validate all dependent steps
        for step_idx in start_step_idx..self.steps.len() {
            let step_name = self.step_name(step_idx).to_owned();
            self.steps_by_name.insert(step_name, step_idx);
            if let Err(diag) = self.steps[step_idx].step.validate(step_idx, self) {
                self.steps[step_idx].step.push_diag(diag);
            }
        }
    }

    // /// Creates a new proof worksheet with the given source text
    // fn new(db: &Database, source: String) -> Self {
    //     let mut worksheet = ProofWorksheet {
    //         source: source.clone(),
    //         db: db.clone(),
    //         ..Default::default()
    //     };
    //     let mut diags = vec![];
    //     let mut steps_by_name = HashMap::new();
    //     let mut steps_iter = source.steps_iter(..);
    //     if steps_iter.next().is_none() || worksheet.update_first_line().is_none() {
    //         diags.push(("".to_string(), Diag::UnparseableFirstLine));
    //     }

    //     let mut steps_names = vec![];
    //     for step_string in steps_iter {
    //         log::info!("Found step: {}", step_string);
    //         let (step, step_diags) = Step::from_str(&step_string, &db);
    //         for (hyp_name, range) in step.hyps() {
    //             if let Some(name) = hyp_name {
    //                 if !steps_names.contains(name) {
    //                     diags.push((
    //                         step.name().to_string(),
    //                         Diag::UnknownStepLabel(range.clone()),
    //                     ));
    //                 }
    //                 steps_names.push(name.to_string());
    //             }
    //         }
    //         for diag in step_diags {
    //             diags.push((step.name().to_string(), diag));
    //         }
    //         steps_by_name.insert(step.name().to_string(), step);
    //     }
    //     worksheet.diags = Arc::new(diags);
    //     worksheet.steps_by_name = Arc::new(steps_by_name);
    //     worksheet
    // }

    pub fn diagnostics(&self) -> Vec<LspDiagnostic> {
        let mut diagnostics = vec![];
        for step_info in self.steps.iter() {
            for diag in step_info.step.diags() {
                let range = diag.get_range(step_info);
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

    pub(crate) fn step_name(&self, step_idx: usize) -> &str {
        let step_info = &self.steps[step_idx];
        step_info.step.name_span().as_ref(&step_info.source)
    }

    pub(crate) fn step_label(&self, step_idx: StepIdx) -> TokenPtr<'_> {
        let step_info = &self.steps[step_idx];
        step_info.step.label(&step_info.source).as_bytes()
    }

    pub(crate) fn hyp_name(&self, step_idx: StepIdx, hyp_idx: usize) -> &str {
        let step_info = &self.steps[step_idx];
        step_info
            .step
            .hyp_ref_span(hyp_idx)
            .as_ref(&step_info.source)
    }

    pub(crate) fn step_formula(&self, step_idx: StepIdx) -> Option<&Formula> {
        let step_info = &self.steps[step_idx];
        step_info.step.formula()
    }

    pub(crate) fn step_stmt_formula(&self, step_idx: StepIdx) -> Result<&Formula, Diag> {
        let unknown_theorem =
            || Diag::UnknownTheoremLabel(self.steps[step_idx].step.name_span().into());
        let label_name = self.step_label(step_idx);
        let sref = self.db.statement(label_name).ok_or_else(unknown_theorem)?;
        self.db
            .stmt_parse_result()
            .get_formula(&sref)
            .ok_or_else(unknown_theorem)
    }

    // First line has changed, update theorem name, loc_after
    fn update_first_line(&mut self) -> Option<()> {
        lazy_static! {
            static ref FIRST_LINE: Regex = Regex::new(
                r"^\$\( <MM> <PROOF_ASST> THEOREM=([0-9A-Za-z_\-\.]+)  LOC_AFTER=(\?|[0-9A-Za-z_\-\.]+)",
            ).unwrap();
        }
        let eol = memchr::memchr(b'\n', self.top.as_bytes()).unwrap_or(self.top.len());
        let first_line = &self.top[0..eol];
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
