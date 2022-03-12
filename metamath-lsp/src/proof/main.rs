mod rope;
mod step;

use crate::worksheet::ProofWorksheet;
use crate::rope::Rope;
use clap::{arg, command};
use log::*;
use metamath_knife::database::DbOptions;
use metamath_knife::Database;
use simple_logger::SimpleLogger;
use std::fs::File;
use std::str::FromStr;

/// Main entry point for the Metamath MMP tool.
pub fn main() {
    let matches = command!()
        .args(&[
            arg!(<DATABASE> "Database file to load"),
            arg!(<PROOF_FILE> "Proof file to load"),
            arg!(-j --jobs <jobs> "Number of threads to use for startup parsing"),
            arg!(-d --debug
            "Activate debug logs, including for the grammar building and statement parsing"),
        ])
        .get_matches();

    let level = if matches.is_present("debug") {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    std::env::set_var("RUST_BACKTRACE", "1");
    let db_file_name = matches
        .value_of("DATABASE")
        .expect("Please provide a database file name");
    let proof_file_name = matches
        .value_of("PROOF_FILE")
        .expect("Please provide a proof file name");
    let options = DbOptions {
        incremental: true,
        autosplit: false,
        jobs: usize::from_str(matches.value_of("jobs").unwrap_or("1")).unwrap_or(1),
        ..Default::default()
    };

    // Build the Database
    info!("Parsing database {}...", db_file_name);
    let mut db = Database::new(options);
    db.parse(db_file_name.into(), Vec::new());
    db.name_pass();
    db.scope_pass();
    db.stmt_parse_pass();

    // Load the MMP File
    SimpleLogger::new()
        .with_utc_timestamps()
        .with_level(level)
        .init()
        .unwrap();
    info!("Loading proof file {}...", proof_file_name);
    let file = File::open(proof_file_name).expect("Could not open proof file");
    let source = Rope::from_reader(file).expect("Could not read proof file");

    // Parse the MMP File
    info!("Parsing proof file {}...", proof_file_name);
    let _worksheet = ProofWorksheet::new(db, &source);
}
