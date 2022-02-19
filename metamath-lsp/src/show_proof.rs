//! Handles shoe proof requests

use crate::vfs::Vfs;
use crate::ServerError;
use metamath_knife::Database;

pub(crate) fn show_proof(
    label: String,
    _vfs: &Vfs,
    db: Database,
) -> Result<Option<String>, ServerError> {
    if let Some(stmt) = db.statement(label.as_bytes()) {
        let mut buf = Vec::new();
        db.export_mmp(stmt, &mut buf)?;
        Ok(Some(
            std::str::from_utf8(buf.as_slice()).unwrap().to_string(),
        ))
    } else {
        Ok(None)
    }
}
