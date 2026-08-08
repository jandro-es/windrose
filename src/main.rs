//! Windrose — discover and assess every AI option on your Mac.
//!
//! `main` only parses arguments and dispatches. All real work lives in the
//! UI-agnostic core modules; formatting for humans lives in `report` and `tui`.

mod cli;
mod doctor;
mod hardware;
mod model;
mod probes;
mod report;
mod scoring;
mod sys;
mod tui;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();

    // No subcommand means the interactive dashboard.
    let command = cli.command.unwrap_or(Command::Tui);

    match command {
        Command::Tui => {
            if let Err(message) = tui::run() {
                eprintln!("{message}");
                std::process::exit(2);
            }
        }
        Command::GenMan { out_dir } => {
            if let Err(message) = cli::gen_man(&out_dir) {
                eprintln!("{message}");
                std::process::exit(2);
            }
        }
        command => {
            if let Err(message) = cli::run(command, cli.json) {
                eprintln!("{message}");
                std::process::exit(2);
            }
        }
    }
}
