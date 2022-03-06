//! LSP Server Implementation, responsible for dispatching LSP
//! requests/replies and notifications back to the client.

use crate::definition::definition;
use crate::diag::make_lsp_diagnostic;
use crate::hover::hover;
use crate::outline::outline;
use crate::references::references;
use crate::show_proof::show_proof;
use crate::vfs::FileContents;
use crate::vfs::Vfs;
use crate::Result;
use crate::ServerError;
use crossbeam::channel::RecvError;
use lazy_static::lazy_static;
use log::*;
use lsp_server::{
    Connection, ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError,
};
use lsp_types::*;
use metamath_knife::diag::DiagnosticClass;
use metamath_knife::{database::DbOptions, Database};
use serde::ser::Serialize;
use serde_json::{from_value, to_value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
enum RequestType {
    Completion(CompletionParams),
    CompletionResolve(Box<CompletionItem>),
    Hover(TextDocumentPositionParams),
    Definition(TextDocumentPositionParams),
    DocumentSymbol(DocumentSymbolParams),
    References(ReferenceParams),
    DocumentHighlight(DocumentHighlightParams),
    ShowProof(String),
}

fn parse_request(
    Request { id, method, params }: Request,
) -> Result<Option<(RequestId, RequestType)>> {
    Ok(match method.as_str() {
        "textDocument/completion" => Some((id, RequestType::Completion(from_value(params)?))),
        "completionItem/resolve" => Some((id, RequestType::CompletionResolve(from_value(params)?))),
        "textDocument/hover" => Some((id, RequestType::Hover(from_value(params)?))),
        "textDocument/definition" => Some((id, RequestType::Definition(from_value(params)?))),
        "textDocument/documentSymbol" => {
            Some((id, RequestType::DocumentSymbol(from_value(params)?)))
        }
        "textDocument/references" => Some((id, RequestType::References(from_value(params)?))),
        "textDocument/documentHighlight" => {
            Some((id, RequestType::DocumentHighlight(from_value(params)?)))
        }
        "metamath/showProof" => Some((id, RequestType::ShowProof(from_value(params)?))),
        _ => None,
    })
}

/// Bitmask of allowed whitespace characters.
///
/// A Metamath database is required to consist of graphic characters, SP, HT,
/// NL, FF, and CR.
const MM_VALID_SPACES: u64 =
    (1u64 << 9) | (1u64 << 10) | (1u64 << 12) | (1u64 << 13) | (1u64 << 32);

/// Check if a character which is known to be <= 32 is a valid Metamath
/// whitespace.  May panic if out of range.
const fn is_mm_space_c0(byte: u8) -> bool {
    (MM_VALID_SPACES & (1u64 << byte)) != 0
}

/// Check if a character is valid Metamath whitespace.
///
/// We generally accept any C0 control as whitespace, with a diagnostic; this
/// function only tests for fully legal whitespace though.
const fn is_mm_space(byte: u8) -> bool {
    byte <= 32 && is_mm_space_c0(byte)
}

#[inline]
/// Check whether a character can be part of a math token
/// This method is used in the "hover" functionality, therefore it needs to isolate both math tokens and labels.
/// The ':' character is excluded here because it is used as a separator in MMP files.
fn is_token_char(c: char) -> bool {
    !c.is_ascii() || (!is_mm_space(c as u8) && c != ':')
}

/// Check if a character can be part of a Metamath label.
fn is_label_char(c: char) -> bool {
    c == '.'
        || c == '-'
        || c == '_'
        || ('a'..='z').contains(&c)
        || ('0'..='9').contains(&c)
        || ('A'..='Z').contains(&c)
}

/// Attempts to find a word around the given position
pub fn word_at(pos: Position, source: FileContents) -> (String, Range) {
    let line = source
        .text
        .get_line(pos.line as usize)
        .unwrap_or_else(|| source.text.slice(0..0));
    let mut start = 0;
    let mut end = line.len_chars() as u32;
    for (idx, ch) in line.chars().enumerate() {
        if !is_token_char(ch) {
            if idx < pos.character as usize {
                start = (idx + 1) as u32;
            } else {
                end = idx as u32;
                break;
            }
        }
    }
    (
        line.get_slice(start as usize..end as usize)
            .unwrap()
            .to_string(),
        Range::new(Position::new(pos.line, start), Position::new(pos.line, end)),
    )
}

struct RequestHandler {
    id: RequestId,
    //cancel: Arc<AtomicBool>,
}

impl RequestHandler {
    fn response<T: Serialize>(self, resp: Result<T, ServerError>) -> Result<()> {
        SERVER.response(self.id, resp.map_err(|e| e.into()))
    }

    fn response_err(self, code: ErrorCode, message: impl Into<String>) -> Result<()> {
        SERVER.response::<Hover>(
            self.id,
            Err(ResponseError {
                code: code as i32,
                message: message.into(),
                data: None,
            }),
        )
    }

    fn handle(self, req: RequestType) -> Result<()> {
        let db = SERVER
            .workspace
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .db
            .clone();
        let vfs = &SERVER.vfs;
        match req {
            RequestType::Hover(TextDocumentPositionParams {
                text_document: doc,
                position,
            }) => self.response(hover(doc.uri.into(), position, vfs, db)),
            RequestType::Definition(TextDocumentPositionParams {
                text_document: doc,
                position,
            }) => self.response(definition(doc.uri.into(), position, vfs, db)),
            RequestType::ShowProof(label) => self.response(show_proof(label, vfs, db)),
            RequestType::References(ReferenceParams {
                text_document_position:
                    TextDocumentPositionParams {
                        text_document: doc,
                        position,
                    },
                work_done_progress_params: _,
                partial_result_params: _,
                context:
                    ReferenceContext {
                        include_declaration: _,
                    },
            }) => self.response(references(doc.uri.into(), position, vfs, db)),
            RequestType::DocumentSymbol(DocumentSymbolParams { .. }) => {
                self.response(outline(vfs, &db))
            }
            _ => self.response_err(ErrorCode::MethodNotFound, "Not implemented"),
        }
    }
}

impl From<ServerError> for ResponseError {
    fn from(e: ServerError) -> ResponseError {
        ResponseError {
            code: ErrorCode::InternalError as i32,
            message: e.0.to_string(),
            data: None,
        }
    }
}

lazy_static! {
    pub static ref SERVER: Server = Server::new();
}

pub struct Workspace {
    db: Database,
    diags: HashMap<Url, Vec<Diagnostic>>,
}

pub struct Server {
    pub workspace: Arc<Mutex<Option<Workspace>>>,
    pub vfs: Vfs,
    pub conn: Connection,
}

impl Server {
    fn new() -> Self {
        let (conn, _iot) = Connection::stdio();
        Server {
            workspace: Arc::default(),
            vfs: Vfs::default(),
            conn,
        }
    }

    pub fn init(&self, options: DbOptions, file_name: &str) {
        let mut db = Database::new(options);
        db.parse(file_name.into(), Vec::new());
        db.name_pass();
        db.scope_pass();
        db.outline_pass();
        db.stmt_parse_pass();
        let mm_diags = db.diag_notations(&[
            DiagnosticClass::Parse,
            DiagnosticClass::Scope,
            DiagnosticClass::Verify,
            DiagnosticClass::Grammar,
            DiagnosticClass::StmtParse,
        ]);
        let lsp_diags = db.render_diags(mm_diags, make_lsp_diagnostic);
        let mut diags = HashMap::new();
        for (uri, diag) in lsp_diags.into_iter().flatten() {
            diags.entry(uri).or_insert_with(Vec::new).push(diag);
        }
        *self.workspace.lock().unwrap() = Some(Workspace { db, diags });
        self.log_message("Database loaded.".to_string()).ok();
    }

    pub(crate) fn start(&self) -> Result<()> {
        let _params: InitializeParams =
            from_value(self.conn.initialize(to_value(ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(true.into()),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                // document_highlight_provider: Some(OneOf::Left(true)),
                ..Default::default()
            })?)?)?;
        Ok(())
    }

    fn send_workspace_diagnostics(&self) {
        let guard = self.workspace.lock().unwrap();
        let workspace = guard.as_ref().unwrap();
        for (uri, diagnostics) in &workspace.diags {
            SERVER
                .send_diagnostics(uri.clone(), None, diagnostics.to_vec())
                .ok();
        }
    }

    fn send_message<T: Into<Message>>(&self, t: T) -> Result<()> {
        Ok(self.conn.sender.send(t.into())?)
    }

    fn show_message(&self, typ: MessageType, message: String) -> Result<()> {
        warn!("{:?}: {}", typ, message);
        self.send_message(Notification {
            method: "window/showMessage".to_owned(),
            params: to_value(ShowMessageParams { typ, message })?,
        })
    }

    fn log_message(&self, message: String) -> Result<()> {
        self.send_message(Notification {
            method: "window/logMessage".to_owned(),
            params: to_value(LogMessageParams {
                typ: MessageType::LOG,
                message,
            })?,
        })
    }

    pub(crate) fn send_diagnostics(
        &self,
        uri: Url,
        version: Option<i32>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<()> {
        info!("Sending diagnostics: {:?}", diagnostics);
        self.send_message(Notification {
            method: "textDocument/publishDiagnostics".to_owned(),
            params: to_value(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version,
            })?,
        })
    }

    fn send_config_request(&self) -> Result<()> {
        use lsp_types::request::{Request, WorkspaceConfiguration};
        let params = lsp_types::ConfigurationParams {
            items: vec![lsp_types::ConfigurationItem {
                scope_uri: None,
                section: Some("metamath".to_string()),
            }],
        };
        let req = lsp_server::Request::new(
            RequestId::from("get_config".to_string()),
            WorkspaceConfiguration::METHOD.to_string(),
            params,
        );
        self.send_message(req)
    }

    pub fn run(&self) {
        // We need this to be able to match on the response for the config getter, but
        // we can't use a string slice since lsp_server doesn't export IdRepr
        let get_config_id = lsp_server::RequestId::from(String::from("get_config"));

        // Request the user's initial configuration on startup.
        if let Err(e) = self.send_config_request() {
            error!("Server panicked: {:?}", e);
        }
        // We already opened and parsed the Database, send the corresponding diagnostics
        self.send_workspace_diagnostics();

        loop {
            match (|| -> Result<bool> {
                match self.conn.receiver.recv() {
                    Err(RecvError) => return Ok(true),
                    Ok(Message::Request(req)) => {
                        if self.conn.handle_shutdown(&req)? {
                            return Ok(true);
                        }
                        if let Some((id, req)) = parse_request(req)? {
                            info!("Got request: {:?}", req);
                            let handler = RequestHandler { id };
                            handler.handle(req)?;
                            // Job::RequestHandler(id, Some(Box::new(req))).spawn();
                        }
                    }
                    Ok(Message::Response(resp)) => {
                        if resp.id == get_config_id {
                            if let Some(val) = resp.result {
                                info!("Set Option: {:?}", val);
                                // let [config]: [ServerOptions; 1] = from_value(val)?;
                                // *self.options.ulock() = config;
                            }
                        } else {
                            info!("Got response: {:?}", resp);
                            // let mut caps = caps.ulock();
                            // if caps.reg_id.as_ref().map_or(false, |rid| rid == &resp.id) {
                            // caps.finish_register(&resp);
                            // } else {
                            //     log!("response to unknown request {}", resp.id)
                            // }
                        }
                    }
                    Ok(Message::Notification(notif)) => {
                        #[allow(clippy::wildcard_imports)]
                        use lsp_types::notification::*;
                        match notif.method.as_str() {
                            DidOpenTextDocument::METHOD => {
                                let DidOpenTextDocumentParams { text_document: doc } =
                                    from_value(notif.params)?;
                                let path = doc.uri.into();
                                info!("open {:?}", path);
                                let _vf = self.vfs.open_virt(path, doc.version, doc.text);
                            }
                            // DidChangeTextDocument::METHOD => {
                            //     let DidChangeTextDocumentParams {text_document: doc} = from_value(notif.params)?;
                            //     let path = doc.uri.into();
                            //     info!("open {:?}", path);
                            //     self.vfs.open_virt(path, doc.version, doc.text);
                            // },
                            _ => {
                                info!("Got notification: {:?}", notif);
                            }
                        }
                    }
                }
                Ok(false)
            })() {
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => error!("Server panicked: {:?}", e),
            }
        }
    }

    fn response<T: Serialize>(&self, id: RequestId, resp: Result<T, ResponseError>) -> Result<()> {
        self.conn.sender.send(Message::Response(match resp {
            Ok(val) => Response {
                id,
                result: Some(to_value(val)?),
                error: None,
            },
            Err(e) => Response {
                id,
                result: None,
                error: Some(e),
            },
        }))?;
        Ok(())
    }
}
