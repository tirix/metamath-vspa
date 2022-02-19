//! LSP Server Implementation, responsible for dispatching LSP
//! requests/replies and notifications back to the client.

use lazy_static::lazy_static;
use log::*;
use serde::ser::Serialize;
use std::sync::{Arc, Mutex};
use serde_json::{from_value, to_value};
use crossbeam::channel::{RecvError};
use crate::hover::hover;
use crate::definition::definition;
use crate::show_proof::show_proof;
use crate::vfs::FileContents;
use crate::vfs::Vfs;
use crate::Result;
use crate::ServerError;
//use crate::util::{ArcList, ArcString, BoxError, FileRef, FileSpan, Span, MutexExt, CondvarExt};
use lsp_types::*;
use lsp_server::{Connection, ErrorCode, Message, Notification,
    Request, RequestId, Response, ResponseError};
use metamath_knife::{Database, database::DbOptions};

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

fn parse_request(Request {id, method, params}: Request) -> Result<Option<(RequestId, RequestType)>> {
    Ok(match method.as_str() {
        "textDocument/completion"        => Some((id, RequestType::Completion(from_value(params)?))),
        "completionItem/resolve"         => Some((id, RequestType::CompletionResolve(from_value(params)?))),
        "textDocument/hover"             => Some((id, RequestType::Hover(from_value(params)?))),
        "textDocument/definition"        => Some((id, RequestType::Definition(from_value(params)?))),
        "textDocument/documentSymbol"    => Some((id, RequestType::DocumentSymbol(from_value(params)?))),
        "textDocument/references"        => Some((id, RequestType::References(from_value(params)?))),
        "textDocument/documentHighlight" => Some((id, RequestType::DocumentHighlight(from_value(params)?))),
        "metamath/showProof"             => Some((id, RequestType::ShowProof(from_value(params)?))),
        _ => None
    })
}

#[inline]
/// Whether a character can be part of a label
fn is_label_char(c: char) -> bool {
    c == '.'
    || c == '-'
    || c == '_'
    || ('a'..='z').contains(&c)
    || ('0'..='9').contains(&c)
    || ('A'..='Z').contains(&c)
}

/// Attempts to find a word around the given position
pub fn word_at(pos: Position, text: FileContents) -> (String, Range) {
    let line = text.0.get_line(pos.line as usize).unwrap();
    let mut start = 0;
    let mut end = line.len_chars() as u32;
    for (idx, ch) in line.chars().enumerate() {
        if !is_label_char(ch) { 
            if idx < pos.character as usize { start = (idx + 1) as u32; }
            else { end = idx as u32; break; }
        }
    }
    (
        line.get_slice(start as usize.. end as usize).unwrap().to_string(),
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
        SERVER.response::<Hover>(self.id, Err(ResponseError {code: code as i32, message: message.into(), data: None}))
    }
 
    fn handle(self, req: RequestType) -> Result<()> {
        let db = SERVER.db.lock().unwrap().as_ref().unwrap().clone();
        let vfs = &SERVER.vfs;
        match req {
            RequestType::Hover(TextDocumentPositionParams {text_document: doc, position}) =>
                self.response(hover(doc.uri.into(), position, vfs, db)),
            RequestType::Definition(TextDocumentPositionParams {text_document: doc, position}) =>
                self.response(definition(doc.uri.into(), position, vfs, db)),
            RequestType::ShowProof(label) =>
                self.response(show_proof(label, vfs, db)),
            _ => self.response_err(ErrorCode::MethodNotFound, "Not implemented"),
        }
    }
}

impl From<ServerError> for ResponseError {
    fn from(e: ServerError) -> ResponseError { ResponseError {
        code: ErrorCode::InternalError as i32, 
        message: e.0.to_string(), 
        data: None,
    } }
}

lazy_static! {
    pub static ref SERVER: Server = Server::new();
}

pub struct Server {
    pub db: Arc<Mutex<Option<Database>>>,
    pub vfs: Vfs,
    pub conn: Connection,
}

impl Server {
    fn new() -> Self {
        let (conn, _iot) = Connection::stdio();
        Server {
            db: Arc::default(),
            vfs: Vfs::default(),
            conn,
        }
    }

    pub fn init(&self, options: DbOptions, file_name: &str) {
        let mut db = Database::new(options);
        db.parse(file_name.into(), Vec::new());
        db.name_pass();
        db.scope_pass();
        db.stmt_parse_pass();
        *self.db.lock().unwrap() = Some(db);
        self.log_message("Database loaded.".to_string()).ok();
    }

    pub(crate) fn start(&self) -> Result<()> {
        let _params : InitializeParams = from_value(self.conn.initialize(
            to_value(ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
                hover_provider: Some(true.into()),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                ..Default::default()
            })?
        )?)?;
        Ok(())
    }

    fn send_message<T: Into<Message>>(&self, t: T) -> Result<()> {
        Ok(self.conn.sender.send(t.into())?)
    }
      
    fn show_message(&self, typ: MessageType, message: String) -> Result<()> {
        self.send_message(Notification {
            method: "window/showMessage".to_owned(),
            params: to_value(ShowMessageParams {typ, message})?
        })
    }
    
    fn log_message(&self, message: String) -> Result<()> {
        self.send_message(Notification {
            method: "window/logMessage".to_owned(),
            params: to_value(LogMessageParams {typ: MessageType::LOG, message})?
        })
    }
    
    fn send_diagnostics(&self, uri: Url, version: Option<i32>, diagnostics: Vec<Diagnostic>) -> Result<()> {
        self.send_message(Notification {
            method: "textDocument/publishDiagnostics".to_owned(),
            params: to_value(PublishDiagnosticsParams {uri, diagnostics, version})?
        })
    }
      
    fn send_config_request(&self) -> Result<()> {
        use lsp_types::request::{WorkspaceConfiguration, Request};
        let params = lsp_types::ConfigurationParams {
            items: vec![lsp_types::ConfigurationItem {
                scope_uri: None,
                section: Some("metamath".to_string()),
            }],
        };
        let req = lsp_server::Request::new(
            RequestId::from("get_config".to_string()),
            WorkspaceConfiguration::METHOD.to_string(),
            params
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
    
        loop {
            match (|| -> Result<bool> {
                match self.conn.receiver.recv() {
                    Err(RecvError) => return Ok(true),
                    Ok(Message::Request(req)) => {
                        if self.conn.handle_shutdown(&req)? {
                            return Ok(true)
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
                    #[allow(clippy::wildcard_imports)] use lsp_types::notification::*;
                    match notif.method.as_str() {
                        DidOpenTextDocument::METHOD => {
                            let DidOpenTextDocumentParams {text_document: doc} = from_value(notif.params)?;
                            let path = doc.uri.into();
                            info!("open {:?}", path);
                            let _vf = self.vfs.open_virt(path, doc.version, doc.text);
                        },
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
            Ok(false) => {},
            Err(e) => error!("Server panicked: {:?}", e)
            }
        }
    }

    fn response<T: Serialize>(&self, id: RequestId, resp: Result<T, ResponseError>) -> Result<()> {
        self.conn.sender.send(Message::Response(match resp {
            Ok(val) => Response { id, result: Some(to_value(val)?), error: None },
            Err(e) => Response { id, result: None, error: Some(e) }
        }))?;
        Ok(())
    }
}


  