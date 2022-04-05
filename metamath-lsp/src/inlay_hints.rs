//! Provides inlay hints

use std::sync::Arc;

use crate::rope_ext::RopeExt;
use crate::util::FileRef;
use crate::vfs::FileContents;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::*;
use metamath_knife::as_str;
use metamath_knife::formula::TypeCode;
use metamath_knife::nameck::Atom;
use metamath_knife::nameck::NameReader;
use metamath_knife::nameck::Nameset;
use metamath_knife::outline::OutlineNodeRef;
use metamath_knife::scopeck::Frame;
use metamath_knife::scopeck::Hyp;
use metamath_knife::scopeck::ScopeResult;
use metamath_knife::statement::FilePos;
use metamath_knife::statement::StatementAddress;
use metamath_knife::Comparer;
use metamath_knife::Database;
use metamath_knife::Span;
use metamath_knife::StatementRef;

struct InlayHintContext<'a> {
    wff: TypeCode,
    class: TypeCode,
    essentials: Vec<StatementAddress>,
    var2bit: std::collections::HashMap<Atom, usize>,
    setvars: Vec<usize>,
    reader: NameReader<'a>,
    source: FileContents,
    hints: Vec<InlayHint>,
    scope: &'a Arc<ScopeResult>,
    nset: &'a Arc<Nameset>,
    db: &'a Database,
}

impl<'a> InlayHintContext<'a> {
    fn new(source: FileContents, db: &'a Database) -> Result<Self, ServerError> {
        Ok(Self {
            hints: vec![],
            wff: db
                .name_result()
                .lookup_symbol(b"wff")
                .ok_or("'wff' typecode not found.")?
                .atom,
            class: db
                .name_result()
                .lookup_symbol(b"class")
                .ok_or("'class' typecode not found.")?
                .atom,
            essentials: vec![],
            var2bit: std::collections::HashMap::new(),
            setvars: vec![],
            reader: NameReader::new(db.name_result()),
            nset: db.name_result(),
            scope: db.scope_result(),
            source,
            db,
        })
    }

    fn statement_hints(&mut self, statement: StatementRef<'_>, frame: &'_ Frame) {
        for token in statement.math_iter() {
            if let Some(float) = self.reader.lookup_float(token.slice) {
                if float.typecode_atom != self.wff && float.typecode_atom != self.class {
                    continue;
                }
                let mut label_parts = vec![];
                let mut first = true;
                let symbol = self.reader.lookup_symbol(token.slice).unwrap(); // We know we can unwrap, since the float exists
                if let Some(&bit) = self.var2bit.get(&symbol.atom) {
                    label_parts.push(InlayHintLabelPart {
                        value: "(".to_string(),
                        ..Default::default()
                    });
                    for &other in self.setvars.iter() {
                        if frame.optional_dv[bit].has_bit(other) {
                            continue;
                        }
                        if !first {
                            label_parts.push(InlayHintLabelPart {
                                value: ",".to_string(),
                                ..Default::default()
                            });
                        }
                        label_parts.push(InlayHintLabelPart {
                            value: as_str(self.nset.atom_name(frame.var_list[other])).to_string(),
                            ..Default::default()
                        });
                        first = false;
                    }
                    label_parts.push(InlayHintLabelPart {
                        value: ")".to_string(),
                        ..Default::default()
                    });
                    if !first {
                        self.hints.push(InlayHint {
                            position: self.source.text.byte_to_lsp_position(
                                statement.math_span(token.index()).end as usize,
                            ),
                            label: label_parts.into(),
                            kind: Some(InlayHintKind::PARAMETER),
                            padding_left: Some(false),
                            padding_right: Some(true),
                            text_edits: None,
                            tooltip: None,
                        });
                    }
                }
            }
        }
    }

    fn assertion_hints(&mut self, statement: StatementRef<'_>) {
        if let Some(frame) = self.scope.get(statement.label()) {
            // We now know the frame, which holds the Distinct Variables (DV) information
            self.var2bit.clear();
            self.setvars = vec![];
            for (index, &tokr) in frame.var_list.iter().enumerate() {
                self.var2bit.insert(tokr, index);
                if let Some(var_tc) = self.reader.lookup_float(self.nset.atom_name(tokr)) {
                    if var_tc.typecode_atom != self.wff && var_tc.typecode_atom != self.class {
                        self.setvars.push(index);
                    }
                }
            }
            for hyp in frame.hypotheses.iter() {
                if let Hyp::Essential(sa, _) = hyp {
                    if !self.essentials.contains(sa) {
                        self.statement_hints(self.db.statement_by_address(*sa), frame);
                        self.essentials.push(*sa);
                    }
                }
            }
            self.statement_hints(statement, frame);
        }
    }
}

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
    let url = path.url().clone();
    let source = vfs.source(path)?;
    let first_byte_idx = source.text.lsp_position_to_byte(range.start);
    let last_byte_idx = source.text.lsp_position_to_byte(range.end);
    let first_statement = find_smallest_outline_containing(
        &url,
        first_byte_idx as FilePos,
        OutlineNodeRef::root_node(&db),
        &db,
    )
    .get_statement()
    .address();
    let last_statement = find_smallest_outline_containing(
        &url,
        last_byte_idx as FilePos,
        OutlineNodeRef::root_node(&db),
        &db,
    )
    .get_statement()
    .address();
    let mut context = InlayHintContext::new(source, &db)?;
    if db.lt(&first_statement, &last_statement) {
        for statement in db
            .statements_range_address(first_statement..=last_statement)
            .filter(|s| s.statement_type().is_assertion())
        {
            context.assertion_hints(statement);
        }
    }
    Ok(context.hints)
}
