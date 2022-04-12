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
    let _text = vfs.source(path, &db)?.as_proof()?;
    Ok(TextEdit::new(Range::new(pos, pos), " TEST ".to_string()))
}
