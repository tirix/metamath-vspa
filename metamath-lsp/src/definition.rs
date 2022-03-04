//! Provides hover information

use crate::server::word_at;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_text::RopeExt;
use lsp_types::*;
use metamath_knife::Database;
use metamath_knife::StatementRef;
use metamath_knife::StatementType;
use std::path::PathBuf;

/// Finds the statement to display for a given token or label
pub(crate) fn find_statement<'a>(token: &'a [u8], db: &'a Database) -> Option<StatementRef<'a>> {
    let nset = db.name_result();
    if let Some(stmt) = db.statement(token) {
        // Statement definitions
        Some(stmt)
    } else if let Some(symbol) = nset.lookup_symbol(token) {
        // Math symbols
        let stmt = db.statement_by_address(symbol.address.statement);
        match stmt.statement_type() {
            StatementType::Constant | StatementType::Variable => {
                // TODO - this could be provided as a utility by metamath-knife wihtout having to build a formula
                let grammar = db.grammar_result();
                if let Ok(formula) = grammar.parse_formula(
                    &mut [symbol.atom].into_iter(),
                    &grammar.typecodes(),
                    nset,
                ) {
                    db.statement(nset.atom_name(formula.get_by_path(&[])?))
                } else {
                    Some(stmt)
                }
            }
            _ => Some(stmt),
        }
    } else {
        None
    }
}

pub(crate) fn stmt_range(stmt: StatementRef<'_>, vfs: &Vfs, db: &Database) -> Option<Location> {
    let path: PathBuf = db.statement_source_name(stmt.address()).into();
    let source = vfs.source(path.clone().into()).ok()?;
    let uri = Url::from_file_path(path.canonicalize().ok()?).ok()?;
    let span = stmt.span();
    let range = Range::new(
        source.text.byte_to_lsp_position(span.start as usize),
        source.text.byte_to_lsp_position(span.end as usize),
    );
    Some(Location { uri, range })
}

pub(crate) fn definition(
    path: FileRef,
    pos: Position,
    vfs: &Vfs,
    db: Database,
) -> Result<Option<Location>, ServerError> {
    let text = vfs.source(path)?;
    let (word, _) = word_at(pos, text);
    if let Some(stmt) = find_statement(word.as_bytes(), &db) {
        Ok(stmt_range(stmt, vfs, &db))
    } else {
        Ok(None)
    }
}
