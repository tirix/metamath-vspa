//! Provides Diagnostics
use crate::util::FileRef;
use annotate_snippets::snippet::AnnotationType;
use annotate_snippets::snippet::Slice;
use annotate_snippets::snippet::Snippet;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticRelatedInformation;
use lsp_types::Location;
use lsp_types::Position;
use lsp_types::Range;
use lsp_types::Url;
use std::io::Write;

/// The message to display for a given `Snippet`
fn make_lsp_message<'a>(snippet: &'a Snippet) -> &'a str {
    snippet
        .title
        .as_ref()
        .map(|a| {
            a.label
                .unwrap_or_else(|| match snippet.title.as_ref().unwrap().annotation_type {
                    AnnotationType::Error => "Error",
                    AnnotationType::Warning => "Warning",
                    AnnotationType::Info => "Info",
                    AnnotationType::Note => "Note",
                    AnnotationType::Help => "Help",
                })
        })
        .unwrap_or("Error")
}

/// Translates an `AnnotationType` to a `lsp_types::DiagnosticSeverity`.
fn make_lsp_severity(annotation_type: AnnotationType) -> lsp_types::DiagnosticSeverity {
    match annotation_type {
        AnnotationType::Error => lsp_types::DiagnosticSeverity::ERROR,
        AnnotationType::Warning => lsp_types::DiagnosticSeverity::WARNING,
        AnnotationType::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        AnnotationType::Note | AnnotationType::Help => lsp_types::DiagnosticSeverity::HINT,
    }
}

/// Translates a `Slice` into a `Range`
fn make_lsp_range(slice: &Slice) -> Range {
    let range = slice
        .annotations
        .get(0)
        .map(|a| a.range)
        .unwrap_or_else(|| (0, 0));
    // We also convert from what appears to be 1-based line numbers to 0-based line numbers
    // TODO - this obviously does not handle multiline errors - all errors are only shown on their first line.
    Range {
        start: Position::new((slice.line_start - 1) as u32, range.0 as u32),
        end: Position::new((slice.line_start - 1) as u32, range.1 as u32),
    }
}

/// Returns the URL for the given slice's file
fn make_lsp_url(slice: &Slice) -> Option<Url> {
    let file_ref: FileRef = slice.origin?.into();
    Some(file_ref.url().clone())
}

/// Translates a `Snippet` to a `Diagnostic`.
pub(crate) fn make_lsp_diagnostic(snippet: Snippet) -> Option<(Url, Diagnostic)> {
    let message = make_lsp_message(&snippet).into();
    let url = make_lsp_url(snippet.slices.get(0)?)?;
    let primary_slice_range = make_lsp_range(snippet.slices.get(0)?);
    let related_information = snippet
        .slices
        .into_iter()
        .filter_map(|slice| {
            let uri = make_lsp_url(&slice)?;
            let range = make_lsp_range(&slice);
            let mut message = Vec::new();
            for ann in slice.annotations {
                writeln!(message, "{}", ann.label).ok();
            }
            Some(DiagnosticRelatedInformation {
                location: Location { uri, range },
                message: String::from_utf8(message).unwrap(),
            })
        })
        .collect::<Vec<DiagnosticRelatedInformation>>();

    Some((
        url,
        Diagnostic {
            message,
            range: primary_slice_range,
            severity: snippet.title.map(|a| make_lsp_severity(a.annotation_type)),
            related_information: if related_information.is_empty() {
                None
            } else {
                Some(related_information)
            },
            ..Diagnostic::default()
        },
    ))
}
