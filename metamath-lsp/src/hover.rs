//! Provides hover information

use crate::definition::find_statement;
use crate::server::word_at;
use crate::util::FileRef;
use crate::vfs::Vfs;
use crate::ServerError;
use lsp_types::*;
use metamath_knife::comment_parser::CommentItem;
use metamath_knife::statement::as_str;
use metamath_knife::statement::StatementRef;
use metamath_knife::Database;
use std::fmt::Write;

fn comment_markup_format(
    stmt: StatementRef<'_>,
    db: &Database,
) -> Result<MarkedString, ServerError> {
    let mut out = String::new();
    writeln!(out, "## {}", as_str(stmt.label()))?;
    let buf = &stmt.segment().segment.buffer;
    if stmt.statement_type().is_assertion() {
        writeln!(out, "```metamath")?;
        // Hypotheses
        for (label, _) in db
            .scope_result()
            .get(stmt.label())
            .ok_or_else(|| ServerError::from("Frame not found"))?
            .as_ref(db)
            .essentials()
        {
            let hyp_stmt = db.statement_by_label(label).unwrap();
            write!(out, "{} ", as_str(hyp_stmt.label()))?;
            for token in hyp_stmt.math_iter() {
                write!(out, "{} ", as_str(&token))?;
            }
            writeln!(out)?;
        }

        // Statement assertion
        write!(out, "{}   ", as_str(stmt.label()))?;
        for token in stmt.math_iter() {
            write!(out, "{} ", as_str(&token))?;
        }
        writeln!(out, "\n```")?;
    }

    if let Some(comment_stmt) = stmt.associated_comment() {
        writeln!(out, "---")?;
        for item in comment_stmt.comment_parser() {
            match item {
                CommentItem::Text(span) => write!(out, "{}", as_str(span.as_ref(buf)))?,
                CommentItem::LineBreak(_) => writeln!(out, "\n")?,
                CommentItem::StartMathMode(_) => write!(out, "`")?,
                CommentItem::EndMathMode(_) => write!(out, " `")?,
                CommentItem::MathToken(span) => write!(out, " {}", as_str(span.as_ref(buf)))?,
                CommentItem::Label(_, span) => write!(out, "`~ {}`", as_str(span.as_ref(buf)))?,
                CommentItem::Url(_, span) => {
                    write!(out, "[{url}]({url})", url = as_str(span.as_ref(buf)))?
                }
                CommentItem::StartHtml(_) => {}
                CommentItem::EndHtml(_) => {}
                CommentItem::StartSubscript(_) => {}
                CommentItem::EndSubscript(_) => {}
                CommentItem::StartItalic(_) => write!(out, " _")?,
                CommentItem::EndItalic(_) => write!(out, "_ ")?,
                CommentItem::BibTag(span) => write!(out, "[{}]", as_str(span.as_ref(buf)))?,
            }
        }
    }
    Ok(MarkedString::String(out))
}

pub(crate) fn hover(
    path: FileRef,
    pos: Position,
    vfs: &Vfs,
    db: Database,
) -> Result<Option<Hover>, ServerError> {
    let text = vfs.source(path, &db)?;
    let (word, range) = word_at(pos, text);
    if let Some(stmt) = find_statement(word.as_bytes(), &db) {
        Ok(Some(Hover {
            range: Some(range),
            contents: HoverContents::Scalar(comment_markup_format(stmt, &db)?),
        }))
    //    } else if let Some(token) = db.name_pass().lookup_symbol() {
    //
    } else {
        Ok(None)
    }
}
