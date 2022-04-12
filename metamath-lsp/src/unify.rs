use lsp_types::{Position, Location, TextEdit, Range};
use metamath_knife::Database;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;


pub(crate) fn unify(
    path: FileRef,
    pos: Position,
    vfs: &Vfs,
    db: Database,
) -> Result<TextEdit, ServerError> {
    let text = vfs.source(path, &db)?.as_proof()?;
    Ok(TextEdit::new(Range::new(pos,pos), " TEST ".to_string()))
}
