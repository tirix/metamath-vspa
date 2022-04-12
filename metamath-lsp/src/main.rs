#![allow(dead_code)]

mod definition;
mod diag;
mod hover;
mod inlay_hints;
mod outline;
mod proof;
mod references;
mod rope_ext;
mod server;
mod show_proof;
mod types;
mod unify;
mod util;
mod vfs;

use crate::server::SERVER;
use clap::{App, Arg};
use crossbeam::channel::SendError;
use log::*;
use std::error::Error;
use std::str::FromStr;
//use crate::util::{ArcList, ArcString, BoxError, FileRef, FileSpan, Span, MutexExt, CondvarExt};
use lsp_server::ProtocolError;
use metamath_knife::database::DbOptions;
use metamath_knife::export::ExportError;

/// Newtype for `Box<dyn Error + Send + Sync>`
pub type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
struct ServerError(BoxError);

type Result<T, E = ServerError> = std::result::Result<T, E>;

impl From<serde_json::Error> for ServerError {
    fn from(e: serde_json::error::Error) -> Self {
        ServerError(Box::new(e))
    }
}

impl From<ProtocolError> for ServerError {
    fn from(e: ProtocolError) -> Self {
        ServerError(Box::new(e))
    }
}

impl From<ExportError> for ServerError {
    fn from(e: ExportError) -> Self {
        ServerError(Box::new(e))
    }
}

impl<T: Send + Sync + 'static> From<SendError<T>> for ServerError {
    fn from(e: SendError<T>) -> Self {
        ServerError(Box::new(e))
    }
}

impl From<&'static str> for ServerError {
    fn from(e: &'static str) -> Self {
        ServerError(e.into())
    }
}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self {
        ServerError(Box::new(e))
    }
}

impl From<std::fmt::Error> for ServerError {
    fn from(e: std::fmt::Error) -> Self {
        ServerError(Box::new(e))
    }
}

impl From<BoxError> for ServerError {
    fn from(e: BoxError) -> Self {
        ServerError(e)
    }
}

impl From<String> for ServerError {
    fn from(e: String) -> Self {
        ServerError(e.into())
    }
}

impl From<()> for ServerError {
    fn from(_: ()) -> Self {
        "Internal Error".into()
    }
}

fn positive_integer(val: String) -> Result<(), String> {
    u32::from_str(&val)
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

/// Extension trait for [`Mutex`](std::sync::Mutex)`<T>`.
pub trait MutexExt<T> {
    /// Like `lock`, but propagates instead of catches panics.
    fn ulock(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    fn ulock(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().expect("propagating poisoned mutex")
    }
}
/// Extension trait for [`Condvar`](std::sync::Condvar).
pub trait CondvarExt {
    /// Like `wait`, but propagates instead of catches panics.
    fn uwait<'a, T>(&self, g: std::sync::MutexGuard<'a, T>) -> std::sync::MutexGuard<'a, T>;
}

impl CondvarExt for std::sync::Condvar {
    fn uwait<'a, T>(&self, g: std::sync::MutexGuard<'a, T>) -> std::sync::MutexGuard<'a, T> {
        self.wait(g).expect("propagating poisoned mutex")
    }
}

// fn setup_log(debug: bool) {
//     let level = if debug {
//         LevelFilter::Debug
//     } else {
//         LevelFilter::Info
//     };
//     use {
//         simplelog::{Config, WriteLogger},
//         std::fs::File,
//     };
//     std::env::set_var("RUST_BACKTRACE", "1");
//     if let Ok(f) = File::create("lsp.log") {
//         let _ = WriteLogger::init(level, Config::default(), f);
//     }
// }

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
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::with_name("database")
                .help("Sets the main metamath file to use")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("jobs")
                .help("Number of threads to use for startup parsing")
                .long("jobs")
                .short("j")
                .takes_value(true)
                .validator(positive_integer),
        )
        .get_matches();
    // setup_log(matches.is_present("debug"));
    let db_file_name = matches.value_of("database").unwrap_or("None");
    let job_count = usize::from_str(matches.value_of("jobs").unwrap_or("1"))
        .expect("validator should check this");
    info!("Parsing database {}", db_file_name);
    let options = DbOptions {
        incremental: true,
        autosplit: false,
        jobs: job_count,
        ..Default::default()
    };
    SERVER.init(options, db_file_name);

    info!("Starting server");
    if let Err(e) = SERVER.start() {
        error!("Error when starting server: {:?}", e);
    } else {
        info!("Started server");
        SERVER.run();
    }
}
