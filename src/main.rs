//! Windrose — discover and assess every AI option on your Mac.
//!
//! `main` only parses arguments and dispatches. All real work lives in the
//! UI-agnostic core modules; formatting for humans lives in `report` and `tui`.

mod cli;
mod hardware;
mod model;
mod probes;
mod sys;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();

    // No subcommand means the interactive dashboard.
    match cli.command.unwrap_or(Command::Tui) {
        Command::Scan => todo!("scan: implemented in Task 11"),
        Command::Score => todo!("score: implemented in Task 11"),
        Command::Doctor => todo!("doctor: implemented in Task 11"),
        Command::Report { format } => todo!("report --format {format}: implemented in Task 11"),
        Command::Tui => todo!("tui: implemented in Task 12"),
        Command::GenMan { out_dir } => {
            todo!("gen-man into {}: implemented in Task 15", out_dir.display())
        }
    }
}
