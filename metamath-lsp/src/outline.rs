//! Provides the database outline : chapters and sections

// use crate::definition::stmt_range;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::*;
use metamath_knife::outline::OutlineNodeRef;
use metamath_knife::parser::HeadingLevel;
use metamath_knife::Database;

#[allow(deprecated)] // workaround rust#60681 - ironically, the field "deprecated" is deprecated.
/// Returns the LSP document symbol for a given outline
fn document_symbol(
    node: OutlineNodeRef,
    include_statements: bool,
    _vfs: &Vfs,
    _db: &Database,
) -> Option<DocumentSymbol> {
    // let stmt = match node {
    //     OutlineNodeRef::Chapter { node_id, .. } => {
    //         let sadd = db.outline_result().tree.get(node_id).stmt_address;
    //         db.statement_by_address(sadd)
    //     },
    //     OutlineNodeRef::Statement { sref, .. } => sref
    // };
    // let range = stmt_range(stmt, vfs, db)?;
    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
    Some(DocumentSymbol {
        name: node.get_name().to_string(),
        detail: None,
        kind: match node.get_level() {
            HeadingLevel::Database => SymbolKind::FILE,
            HeadingLevel::MajorPart => SymbolKind::MODULE,
            HeadingLevel::Section => SymbolKind::NAMESPACE,
            HeadingLevel::SubSection => SymbolKind::PACKAGE,
            HeadingLevel::SubSubSection => SymbolKind::CLASS,
            HeadingLevel::Statement => SymbolKind::METHOD,
        },
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: Some(children_symbols(node, include_statements, _vfs, _db)),
    })
}

/// Returns a list of all children nodes of `node`, as LSP `DocumentSymbol`
fn children_symbols(
    node: OutlineNodeRef,
    include_statements: bool,
    vfs: &Vfs,
    db: &Database,
) -> Vec<DocumentSymbol> {
    let mut chapters = vec![];
    for chapter in node.children_iter() {
        if include_statements || chapter.get_level() != HeadingLevel::Statement {
            if let Some(symbol) = document_symbol(chapter, include_statements, vfs, db) {
                chapters.push(symbol);
            }
        }
    }
    chapters
}

/// Builds the Outline
pub(crate) fn outline(vfs: &Vfs, db: &Database) -> Result<Vec<DocumentSymbol>, ServerError> {
    let root = OutlineNodeRef::root_node(db);
    Ok(children_symbols(root, false, vfs, db))
}
