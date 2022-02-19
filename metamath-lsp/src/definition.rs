//! Provides hover information

use crate::server::word_at;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_text::RopeExt;
use lsp_types::*;
use metamath_knife::Database;
use std::path::PathBuf;

pub(crate) fn definition(
    path: FileRef,
    pos: Position,
    vfs: &Vfs,
    db: Database,
) -> Result<Option<Location>, ServerError> {
    let text = vfs.source(&path);
    let (word, _) = word_at(pos, text);
    if let Some(stmt) = db.statement(word.as_bytes()) {
        let path: PathBuf = db.statement_source_name(stmt.address()).into();
        let text = vfs.source(&path.clone().into());
        let uri = Url::from_file_path(path.canonicalize()?)?;
        let span = stmt.span();
        let range = Range::new(
            text.0.byte_to_lsp_position(span.start as usize),
            text.0.byte_to_lsp_position(span.end as usize),
        );
        Ok(Some(Location { uri, range }))
    //    } else if let Some(token) = db.name_pass().lookup_symbol {
    //
    } else {
        Ok(None)
    }
}
