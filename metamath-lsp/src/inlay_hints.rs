//! Provides inlay hints

use crate::rope_ext::RopeExt;
use crate::server::SERVER;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::*;
use metamath_knife::Database;
use metamath_knife::Comparer;
use metamath_knife::Span;
use metamath_knife::outline::OutlineNodeRef;
use metamath_knife::statement::FilePos;

/// Returns the smallest outline containing the given position,
/// within the provided outline.
pub(crate) fn find_smallest_outline_containing<'a>(
    url: &'a Url,
    byte_idx: FilePos,
    outline: OutlineNodeRef<'a>,
    db: &'_ Database,
) -> OutlineNodeRef<'a> {
    let mut last_span = Span::NULL;
    for child_outline in outline.children_iter() {
        let span = child_outline.get_span(); // db.statement_span(child_outline.get_statement());
        SERVER.log_message(format!("Checking {}: {} in {:?}", child_outline, byte_idx, span)).ok();
        if (span.start..span.end).contains(&byte_idx) || byte_idx <= last_span.end {
            return find_smallest_outline_containing(url, byte_idx, child_outline, db);
        }
        last_span = span;
    }
    outline
}

pub(crate) fn inlay_hints(
    path: FileRef,
    range: Range,
    vfs: &Vfs,
    db: Database,
) -> Result<Vec<InlayHint>, ServerError> {
    let mut hints = vec![];
    let url = path.url().clone();
    let source = vfs.source(path)?;
    let first_byte_idx = source.text.lsp_position_to_byte(range.start);
    let last_byte_idx = source.text.lsp_position_to_byte(range.end);
    let first_statement = find_smallest_outline_containing(&url, first_byte_idx as FilePos, OutlineNodeRef::root_node(&db), &db).get_statement().address();
    let last_statement = find_smallest_outline_containing(&url, last_byte_idx as FilePos, OutlineNodeRef::root_node(&db), &db).get_statement().address();
    if db.lt(&first_statement, &last_statement) {
        for statement in db.statements_range_address(first_statement..=last_statement).filter(|s| s.statement_type().takes_label()) {
            hints.push(InlayHint {
                position: source.text.byte_to_lsp_position(statement.label_span().end as usize),
                label:InlayHintLabel::String("(test)".to_owned()),
                kind:Some(InlayHintKind::PARAMETER),
                padding_left:Some(false),
                padding_right:Some(true), 
                text_edits: None, 
                tooltip: None // 
            });
        }
    }
    SERVER.log_message(format!("InlayHint range: {:?} -> {} hints", range, hints.len()).into()).ok();
    Ok(hints)
}
