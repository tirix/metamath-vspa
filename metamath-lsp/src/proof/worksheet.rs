use crate::proof::step::Step;
use lazy_static::lazy_static;
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range as LspRange,
    TextDocumentContentChangeEvent,
};
use metamath_knife::diag::StmtParseError;
use metamath_knife::statement::StatementAddress;
use metamath_knife::{Database, StatementRef};
use regex::{Match, Regex};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::{Index, Range};

/// A Diagnostic
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub(crate) struct Span(Range<usize>);

impl From<Match<'_>> for Span {
    fn from(m: Match) -> Self {
        Self(m.range())
    }
}

impl Span {
    #[inline]
    #[must_use]
    pub fn until(end: usize) -> Self {
        Self(0..end)
    }

    #[inline]
    #[must_use]
    pub fn into_ref(self, buf: &str) -> &str {
        &buf[self.0]
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

    fn get_range(&self, step_info: &StepInfo) -> Range<usize> {
        let step_span = step_info.byte_idx..step_info.byte_idx + step_info.source.len();
        match self {
            Diag::UnknownStepLabel(range) | Diag::UnknownTheoremLabel(range) => Range {
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
            | Diag::DatabaseDiagnostic(StmtParseError::ParsedStatementWrongTypeCode(_)) => {
                step_info.step.formula_range(step_info.byte_idx)
            }
        }
    }
}

/// Identifies a proof step in a worksheet.
/// This is internal to the [ProofWorksheet]
type StepIdx = usize;

/// Information relative to a step
/// The "source" string of each step is cloned to be stored in the step info.
#[derive(Debug)]
struct StepInfo {
    byte_idx: usize,
    line_idx: usize,
    source: String,
    step: Step,
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
    db: Database,
    /// The statement which is being proven
    sadd: Option<StatementAddress>,
    /// A position in the database. Only statements before this one are allowed in a proof.
    loc_after: Option<StatementAddress>,
    /// Top line and first comment
    top: String,
    /// All the steps in this proof, in the order they appear
    steps: Vec<StepInfo>,
    /// The indices of the steps in this proof, referenced by their proof label (usually these are actually numbers, but any valid metamath label is allowed)
    steps_by_name: HashMap<String, StepIdx>,
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

    fn step_at_line(&self, line_idx: usize) -> Option<&StepInfo> {
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
            end: self.byte_to_lsp_position(
                self.steps
                    .iter()
                    .last()
                    .map_or(0, |s| s.byte_idx + s.source.len()),
            ),
        });

        // Find out the first step and last step impacted.
        let (first_step_idx, first_byte_idx) = self.step_at(range.start);
        let (last_step_idx, last_byte_idx) = self.step_at(range.end);
        let first_source = first_step_idx.map_or(&self.top, |i| &self.step_info(i).source);
        let last_source = last_step_idx.map_or(&self.top, |i| &self.step_info(i).source);

        // So we can recover the full new text
        let mut new_text = String::new();
        new_text.push_str(Span(0..first_byte_idx).into_ref(first_source));
        new_text.push_str(&change.text);
        new_text.push_str(Span(last_byte_idx..last_source.len()).into_ref(last_source));
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
            let sub_byte_idx = self.steps[end_step_idx].byte_idx
                + self.steps[end_step_idx].source.len()
                - self.steps[start_step_idx].byte_idx;
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

        // Finally, we can replace the new steps into our reference
        self.steps.splice(
            first_step_idx.unwrap_or(0)..last_step_idx.map(|i| i + 1).unwrap_or(0),
            add_steps,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use metamath_knife::database::DbOptions;

    pub(super) fn mkdb(text: &[u8]) -> Database {
        let options = DbOptions {
            incremental: true,
            ..DbOptions::default()
        };
        let mut db = Database::new(options);
        db.parse(
            "test.mm".to_owned(),
            vec![("test.mm".to_owned(), text.to_owned())],
        );
        db.grammar_pass();
        db
    }

    fn mkdiag(
        line_from: u32,
        char_from: u32,
        line_to: u32,
        char_to: u32,
        message: &str,
    ) -> LspDiagnostic {
        LspDiagnostic {
            range: LspRange {
                start: Position {
                    line: line_from,
                    character: char_from,
                },
                end: Position {
                    line: line_to,
                    character: char_to,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            message: message.to_owned(),
            ..LspDiagnostic::default()
        }
    }

    const TEST_DB: &[u8] = b"
        $c |- wff ( ) -> $.
        $( $j syntax 'wff'; syntax '|-' as 'wff'; $)
        $v ph ps ch $.
        wph $f wff ph $.
        wps $f wff ps $.
        wps $f wff ch $.
        wi $a wff ( ph -> ps ) $.
        ${
            min $e |- ph $.
            maj $e |- ( ph -> ps ) $.
            ax-mp $a |- ps $.
        $}
        ax-1 $a |- ( ph -> ( ps -> ph ) ) $.
        ${
            a1i.1 $e |- ph $.
            a1i $p |- ( ps -> ph ) $= ? $.
        $}
    ";

    const TEST_PROOF: &str = "$( <MM> <PROOF_ASST> THEOREM=a1i  LOC_AFTER=?

* Inference introducing an antecedent.  (Contributed by NM, 29-Dec-1992.)

h1::a1i.1      |- ph
2::ax-1        |- ( ph
    -> ( ps -> ph ) )
qed:1,2:ax-mp  |- ( ps -> ph )

$=    ( wi ax-1 ax-mp ) ABADCABEF $.

$)
";

const TEST_PROOF_2: &str = "$( <MM> <PROOF_ASST> THEOREM=a1i  LOC_AFTER=?

* Inference introducing an antecedent.  (Contributed by NM, 29-Dec-1992.)

h1::a1i.1      |- ph
2::ax-1        |- ( ph -> ( ps -> ph ) )
*
* x y
qed:1,2:ax-mp  |- ( ps -> ph )

$=    ( wi ax-1 ax-mp ) ABADCABEF $.

$)
    ";
    
    #[test]
    fn parse_worksheet() {
        let db = &mkdb(TEST_DB);
        let worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
        assert_eq!(worksheet.steps.len(), 3);
        assert_eq!(worksheet.steps[0].line_idx, 4);
        assert_eq!(worksheet.steps[1].line_idx, 5);
        assert_eq!(worksheet.steps[2].line_idx, 7);
        assert_eq!(worksheet.steps[0].byte_idx, 122);
        assert_eq!(worksheet.steps[1].byte_idx, 143);
        assert_eq!(worksheet.steps[2].byte_idx, 188);
        assert!(worksheet.step_at_line(0).is_none());
        assert_eq!(
            worksheet.byte_to_lsp_position(188),
            Position {
                line: 7,
                character: 0
            }
        );
        assert_eq!(
            worksheet.byte_to_lsp_position(200),
            Position {
                line: 7,
                character: 12
            }
        );
        assert_eq!(
            worksheet.lsp_position_to_byte(Position {
                line: 7,
                character: 0
            }),
            188
        );
        assert_eq!(
            worksheet.lsp_position_to_byte(Position {
                line: 7,
                character: 12
            }),
            200
        );
        assert_eq!(
            worksheet.steps[1].source,
            "2::ax-1        |- ( ph\n    -> ( ps -> ph ) )\n"
        );
        assert_eq!(worksheet.line(7), "qed:1,2:ax-mp  |- ( ps -> ph )");
        assert_eq!(worksheet.line(6), "    -> ( ps -> ph ) )");
        println!("{:#?}", worksheet.diagnostics());
        assert_eq!(worksheet.diagnostics(), vec![]);
    }

    #[test]
    fn parse_worksheet_with_comments() {
        let db = &mkdb(TEST_DB);
        let worksheet = ProofWorksheet::from_string(TEST_PROOF_2.to_string(), db).unwrap();
        assert_eq!(worksheet.steps.len(), 3);
        assert_eq!(worksheet.steps[0].line_idx, 4);
        assert_eq!(worksheet.steps[1].line_idx, 5);
        assert_eq!(worksheet.steps[2].line_idx, 8);
        assert_eq!(worksheet.steps[0].byte_idx, 122);
        assert_eq!(worksheet.steps[1].byte_idx, 143);
        assert_eq!(worksheet.steps[2].byte_idx, 192);
        assert_eq!(
            worksheet.steps[1].source,
            "2::ax-1        |- ( ph -> ( ps -> ph ) )\n*\n* x y\n"
        );
        assert_eq!(
            worksheet.steps[2].source,
            "qed:1,2:ax-mp  |- ( ps -> ph )\n\n$=    ( wi ax-1 ax-mp ) ABADCABEF $.\n\n$)\n    "
        );
        assert_eq!(worksheet.line(6), "*");
        assert_eq!(worksheet.line(7), "* x y");
        println!("{:#?}", worksheet.diagnostics());
        assert_eq!(worksheet.diagnostics(), vec![]);
    }

    #[test]
    fn parse_worksheet_diags() {
        let db = &mkdb(TEST_DB);
        let worksheet = ProofWorksheet::from_string(
            "$( <MM> <PROOF_ASST> THEOREM=mp2  LOC_AFTER=?

* A double modus ponens inference.  (Contributed by NM, 5-Apr-1994.)

h1::mp2.1      |- ph
h2::mp2.2      |- ps
h3::mp2.3      |- ( ph -> ( ps -> ch ) )
4:1,3:ax-mp    |- ( ps -> ch )
5x::
6
qed:2,4:ax-mp  |- ch

$=    ( wi ax-mp ) BCEABCGDFHH $.

$)
"
            .to_string(),
            db,
        )
        .unwrap();
        let diags = worksheet.diagnostics();
        println!("{:#?}", diags);
        assert_eq!(diags[0], mkdiag(4, 4, 4, 9, "Unknown theorem"));
        assert_eq!(diags[1], mkdiag(5, 4, 5, 9, "Unknown theorem"));
        assert_eq!(diags[2], mkdiag(6, 4, 6, 9, "Unknown theorem"));
        assert_eq!(diags[3], mkdiag(8, 0, 9, 0, "Could not parse proof line"));
        assert_eq!(diags[4], mkdiag(9, 0, 10, 0, "Could not parse proof line"));
    }

    #[test]
    fn worksheet_insert_new_step() {
        let db = &mkdb(TEST_DB);
        let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
        worksheet.apply_change(&TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: Position {
                    line: 7,
                    character: 0,
                },
                end: Position {
                    line: 7,
                    character: 0,
                },
            }),
            range_length: None,
            text: "3::ax-1 |- ( ch -> ( ps -> ch ) )\n".to_owned(),
        });
        println!("{:#?}", worksheet.steps);
        assert_eq!(worksheet.steps.len(), 4);
        assert_eq!(worksheet.steps[1].line_idx, 5);
        assert_eq!(worksheet.steps[2].line_idx, 7);
        assert_eq!(worksheet.steps[3].line_idx, 8);
        assert_eq!(worksheet.steps[1].byte_idx, 143);
        assert_eq!(worksheet.steps[2].byte_idx, 188);
        assert_eq!(worksheet.steps[3].byte_idx, 222);
        assert_eq!(
            worksheet.steps[2].source,
            "3::ax-1 |- ( ch -> ( ps -> ch ) )\n"
        );
        println!("{:#?}", worksheet.diagnostics());
        assert_eq!(worksheet.diagnostics(), vec![]);
    }

    #[test]
    fn worksheet_insert_middle() {
        let db = &mkdb(TEST_DB);
        let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
        worksheet.apply_change(&TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: Position {
                    line: 5,
                    character: 20,
                },
                end: Position {
                    line: 5,
                    character: 22,
                },
            }),
            range_length: None,
            text: "( ps -> ch )".to_owned(),
        });
        println!("{:#?}", worksheet.steps);
        assert_eq!(worksheet.steps.len(), 3);
        assert_eq!(worksheet.steps[0].line_idx, 4);
        assert_eq!(worksheet.steps[1].line_idx, 5);
        assert_eq!(worksheet.steps[2].line_idx, 7);
        assert_eq!(worksheet.steps[0].byte_idx, 122);
        assert_eq!(worksheet.steps[1].byte_idx, 143);
        assert_eq!(worksheet.steps[2].byte_idx, 198);
        assert_eq!(
            worksheet.steps[1].source,
            "2::ax-1        |- ( ( ps -> ch )\n    -> ( ps -> ph ) )\n"
        );
        println!("{:#?}", worksheet.diagnostics());
        assert_eq!(worksheet.diagnostics(), vec![]);
    }

    #[test]
    fn worksheet_insert_newline_before_blank() {
        let db = &mkdb(TEST_DB);
        let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
        worksheet.apply_change(&TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: Position {
                    line: 6,
                    character: 8,
                },
                end: Position {
                    line: 6,
                    character: 8,
                },
            }),
            range_length: None,
            text: "\n".to_owned(),
        });
        println!("{:#?}", worksheet.steps);
        assert_eq!(worksheet.steps.len(), 3);
        assert_eq!(worksheet.steps[0].line_idx, 4);
        assert_eq!(worksheet.steps[1].line_idx, 5);
        assert_eq!(worksheet.steps[2].line_idx, 8);
        assert_eq!(worksheet.steps[0].byte_idx, 122);
        assert_eq!(worksheet.steps[1].byte_idx, 143);
        assert_eq!(worksheet.steps[2].byte_idx, 189);
        assert_eq!(
            worksheet.steps[1].source,
            "2::ax-1        |- ( ph\n    -> (\n ps -> ph ) )\n"
        );
        println!("{:#?}", worksheet.diagnostics());
        assert_eq!(worksheet.diagnostics(), vec![]);
    }

    #[test]
    fn worksheet_insert_newline_before_char() {
        let db = &mkdb(TEST_DB);
        let mut worksheet = ProofWorksheet::from_string(TEST_PROOF.to_string(), db).unwrap();
        worksheet.apply_change(&TextDocumentContentChangeEvent {
            range: Some(LspRange {
                start: Position {
                    line: 6,
                    character: 9,
                },
                end: Position {
                    line: 6,
                    character: 9,
                },
            }),
            range_length: None,
            text: "\n".to_owned(),
        });
        println!("{:#?}", worksheet.steps);
        assert_eq!(worksheet.steps.len(), 4);
        assert_eq!(worksheet.steps[0].line_idx, 4);
        assert_eq!(worksheet.steps[1].line_idx, 5);
        assert_eq!(worksheet.steps[2].line_idx, 7);
        assert_eq!(worksheet.steps[3].line_idx, 8);
        assert_eq!(worksheet.steps[0].byte_idx, 122);
        assert_eq!(worksheet.steps[1].byte_idx, 143);
        assert_eq!(worksheet.steps[2].byte_idx, 176);
        assert_eq!(worksheet.steps[3].byte_idx, 189);
        assert_eq!(
            worksheet.steps[1].source,
            "2::ax-1        |- ( ph\n    -> ( \n"
        );
        assert_eq!(
            worksheet.steps[2].source,
            "ps -> ph ) )\n"
        );
        let diags = worksheet.diagnostics();
        println!("{:#?}", diags);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0], mkdiag(6, 8, 6, 9, "Parsed statement too short"));
        assert_eq!(diags[1], mkdiag(7, 0, 8, 0, "Could not parse proof line"));
    }
}
