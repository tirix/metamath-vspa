//! Additional types used on the LSP interface

use lsp_types::{Range, TextDocumentIdentifier};
use serde::{Deserialize, Serialize};

/// A parameter literal used in inlay hint requests.
///
/// @since 3.17.0 - proposed state
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowProofParams {
    /// The content currently highlighted.
    pub label: String,

    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The visible document range for which inlay hints should be computed.
    pub range: Range,
}
