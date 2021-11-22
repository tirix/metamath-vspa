mod util;

use log::*;
use clap::{Arg,App};
use std::error::Error;
use serde::ser::Serialize;
use std::sync::atomic::AtomicBool;
use serde_json::{from_value, to_value};
use std::sync::Arc;
use crossbeam::channel::{SendError, RecvError};
use crate::util::FileRef;
//use crate::util::{ArcList, ArcString, BoxError, FileRef, FileSpan, Span, MutexExt, CondvarExt};
use lsp_types::*;
use lsp_server::{Connection, ErrorCode, Message, Notification, ProtocolError,
    Request, RequestId, Response, ResponseError};

/// Newtype for `Box<dyn Error + Send + Sync>`
pub type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
struct ServerError(BoxError);

type Result<T, E = ServerError> = std::result::Result<T, E>;


impl From<serde_json::Error> for ServerError {
    fn from(e: serde_json::error::Error) -> Self { ServerError(Box::new(e)) }
}

impl From<ProtocolError> for ServerError {
    fn from(e: ProtocolError) -> Self { ServerError(Box::new(e)) }
}

impl<T: Send + Sync + 'static> From<SendError<T>> for ServerError {
    fn from(e: SendError<T>) -> Self { ServerError(Box::new(e)) }
}

impl From<&'static str> for ServerError {
    fn from(e: &'static str) -> Self { ServerError(e.into()) }
}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self { ServerError(Box::new(e)) }
}

impl From<BoxError> for ServerError {
    fn from(e: BoxError) -> Self { ServerError(e) }
}

impl From<String> for ServerError {
    fn from(e: String) -> Self { ServerError(e.into()) }
}

#[derive(Debug)]
enum RequestType {
    Completion(CompletionParams),
    CompletionResolve(Box<CompletionItem>),
    Hover(TextDocumentPositionParams),
    Definition(TextDocumentPositionParams),
    DocumentSymbol(DocumentSymbolParams),
    References(ReferenceParams),
    DocumentHighlight(DocumentHighlightParams),
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
        _ => None
    })
}

fn hover(path: FileRef, pos: Position) -> Result<Option<Hover>, ResponseError> {
    let string = MarkedString::String("Aloha".into());
    Ok(Some(Hover {
        range: None, //Some(text.to_range(out[0].0)),
        contents: HoverContents::Scalar(string),
    }))
}

struct RequestHandler {
    id: RequestId,
    //cancel: Arc<AtomicBool>,
}

impl RequestHandler {
    fn response_err(code: ErrorCode, message: impl Into<String>) -> ResponseError {
        ResponseError {code: code as i32, message: message.into(), data: None}
    }
 
    fn handle(self, req: RequestType) -> Result<impl Serialize, ResponseError> {
        match req {
            RequestType::Hover(TextDocumentPositionParams {text_document: doc, position}) =>
                hover(doc.uri.into(), position),
            _ => Err(RequestHandler::response_err(ErrorCode::MethodNotFound, "Not implemented")),
        }
    }
}

struct Server {
    conn: Connection,
}

impl Server {
    fn new() -> Self {
        let (conn, _iot) = Connection::stdio();
        Server {
            conn,
        }
    }

    fn start(&self) -> Result<()> {
        let params : InitializeParams = from_value(self.conn.initialize(
            to_value(ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::Incremental)),
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
      
    fn log_message(&self, message: String) -> Result<()> {
        self.send_message(Notification {
            method: "window/logMessage".to_owned(),
            params: to_value(LogMessageParams {typ: MessageType::Log, message})?
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
    
    fn run(&self) {
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
                            let handler = RequestHandler { id: id.clone() };
                            let resp = handler.handle(req);
                            self.response(id, resp)?;
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
                    info!("Got notification: {:?}", notif);
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


/// Main entry point for the Metamath language server.
///
/// This function sets up an [LSP] connection using stdin and stdout. 
/// This allows for extensions such as [`metamath-vscode`] to use `metamath-lsp`
/// as a language server.
///
/// # Arguments
///
/// `metamath-lsp [--debug]`, where:
///
/// - `-d`, `--debug`: enables debugging output to `lsp.log`
///
/// [LSP]: https://microsoft.github.io/language-server-protocol/
/// [`metamath-vscode`]: https://github.com/tirix/vsmmpa/tree/master/metamath-vscode
pub fn main() {
    let matches = App::new("Metamath LSP Server")
        .version("1.0")
        .author("Thierry A.")
        .about("A Metmamath Language Server Protocole implementation")
        .arg(Arg::with_name("debug")
            .short("d")
            .long("debug")
            .help("Sets the level of verbosity")).get_matches();
    if matches.is_present("debug") || true {
        use {simplelog::{Config, LevelFilter, WriteLogger}, std::fs::File};
        std::env::set_var("RUST_BACKTRACE", "1");
        if let Ok(f) = File::create("lsp.log") {
            let _ = WriteLogger::init(LevelFilter::Debug, Config::default(), f);
        }
    }
    let server = Server::new(); 
    info!("Starting server");
    if let Err(e) = server.start() {
        error!("Error when starting server: {:?}", e);
    } else {
        info!("Started server");
        server.run();
    }
}