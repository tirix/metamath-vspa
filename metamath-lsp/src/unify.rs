//! This is handler for the "Unify" command
//! It tries to apply the best fitting tactics, based on the context.
use crate::prover::tactics::Apply;
use crate::prover::tactics::Assumption;
use crate::prover::tactics::Sorry;
use crate::prover::tactics::Try;
use crate::prover::Tactics;
use crate::prover::TacticsList;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::{Position, Range, TextEdit};
use metamath_knife::Database;
use metamath_knife::Label;

pub(crate) fn unify(
    path: FileRef,
    pos: Position,
    vfs: &Vfs,
    db: Database,
) -> Result<TextEdit, ServerError> {
    let source = vfs.source(path, &db)?;
    let worksheet = source.as_proof()?;
    let step_info = worksheet
        .step_at_line(pos.line as usize)
        .ok_or_else(|| ServerError::from("No step to unify"))?;
    let mut context = worksheet.build_context(step_info)?;
    let start_pos = worksheet.byte_to_lsp_position(step_info.byte_idx);
    let end_pos = worksheet.byte_to_lsp_position(step_info.last_byte_idx());
    let proof_step = if let Some(label) = step_info.get_label(&db) {
        // If the step theorem label is known, use the `default` tactics
        // (this ignores the provided hypothesis steps)
        default_tactics_for_theorem(label).elaborate(&mut context)
    } else {
        // If the step theorem label is unknown, use the `assumption` tactics
        default_tactics().elaborate(&mut context)
    };
    let new_text = worksheet.proof_text(&proof_step?, step_info.name());
    Ok(TextEdit::new(Range::new(start_pos, end_pos), new_text))
}

// Returns the default tactics to apply if the theorem statement is known
// This will apply the theorem, and for each of its hypothesis, apply the default tactics
fn default_tactics_for_theorem(label: Label) -> impl Tactics {
    Apply::new(label, TacticsList::Repeat(Box::new(default_tactics())))
}

// Returns the default tactics to apply if the theorem statement is unknown
// This will first try to match the goal with known assumptions, and return a default proof step if it fails
fn default_tactics() -> impl Tactics {
    Try::new(vec![Box::new(Assumption), Box::new(Sorry)])
}
