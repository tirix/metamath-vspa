//! Provides references to a given statement

use crate::definition::find_statement;
use crate::definition::stmt_location;
use crate::server::word_at;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::*;
use metamath_knife::statement::StatementRef;
use metamath_knife::statement::StatementType;
use metamath_knife::Database;
use std::ops::Bound;

pub(crate) fn references(
    path: FileRef,
    pos: Position,
    vfs: &Vfs,
    db: Database,
) -> Result<Vec<Location>, ServerError> {
    let text = vfs.source(path, &db)?;
    let (word, _range) = word_at(pos, text);
    let label = word.as_bytes();
    if let Some(stmt) = find_statement(label, &db) {
        let mut locations = vec![];
        for stmt in db.statements_range_address((Bound::Excluded(stmt.address()), Bound::Unbounded))
        {
            if is_direct_use(&stmt, label) {
                if let Some(location) = stmt_location(stmt, vfs, &db) {
                    locations.push(location);
                }
            }
        }
        Ok(locations)
    } else {
        Ok(Vec::new())
    }
}

fn is_direct_use(stmt: &StatementRef<'_>, label: &[u8]) -> bool {
    stmt.statement_type() == StatementType::Provable && {
        let len = stmt.proof_len();
        if len == 0 || stmt.proof_slice_at(0) != b"(" {
            return false;
        }
        for i in 1..len {
            let ref_stmt = stmt.proof_slice_at(i);
            if ref_stmt == b")" {
                break;
            } else if ref_stmt == label {
                return true;
            }
        }
        false
    }
}
