//! This is handler for the "Unify" command
//! It tries to apply the best fitting tactics, based on the context.
use crate::prover::tactics::Apply;
use crate::prover::tactics::Assumption;
use crate::prover::Tactics;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::{Position, Range, TextEdit};
use metamath_knife::Database;

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
        // If the step theorem label is known, use the `apply` tactics
        // (this ignores the provided hypothesis steps)
        Apply::new(label).elaborate(&mut context)
    } else {
        // If the step theorem label is unknown, use the `assumption` tactics
        Assumption.elaborate(&mut context)
    };
    let new_text = worksheet.proof_text(&proof_step?);
    Ok(TextEdit::new(Range::new(start_pos, end_pos), new_text))
}
