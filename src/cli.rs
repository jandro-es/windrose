//! Command-line surface for Windrose.
//!
//! Every user-facing string here follows the plain-language rule: no unexplained
//! jargon, and any technology named gets a short explanation on first appearance.

use crate::report::{self, ScanResult};
use crate::sys::RealSys;
use clap::{Parser, Subcommand};
use std::io::{IsTerminal, Write};

#[derive(Parser)]
#[command(
    name = "windrose",
    version,
    about = "Discover and assess every AI option on your Mac",
    long_about = "Windrose looks at your Mac and tells you which AI tools you already have, \
                  which ones are ready to use, and how well this machine can run AI models \
                  on its own — without sending anything to the internet.\n\n\
                  Run it with no arguments to open the interactive dashboard."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Print machine-readable JSON instead of friendly text
    #[arg(long, global = true)]
    pub json: bool,

    /// Turn off coloured output (useful when saving output to a file)
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan this Mac and list every AI option found
    Scan,
    /// Score how well this Mac can run AI models on-device
    Score,
    /// Check health, guide setup, and suggest performance fixes
    Doctor,
    /// Write a full report (markdown or JSON) to a file or stdout
    Report {
        /// Output format: "md" for a readable document, "json" for data
        #[arg(long, default_value = "md")]
        format: String,
    },
    /// Open the interactive dashboard (default)
    Tui,
    #[command(hide = true)]
    GenMan { out_dir: std::path::PathBuf },
}

/// Run one of the non-interactive commands.
pub fn run(command: Command, json: bool) -> Result<(), String> {
    match command {
        Command::Scan => {
            let result = report::gather(&RealSys);
            print(&if json {
                report::render_json(&result)
            } else {
                report::render_text(&result)
            });
        }
        Command::Score => {
            let result = report::gather(&RealSys);
            print(&if json {
                report::render_json(&result)
            } else {
                report::render_score(&result)
            });
        }
        Command::Doctor => doctor(json)?,
        Command::Report { format } => {
            let result = report::gather(&RealSys);
            print(&match format.as_str() {
                "json" => report::render_json(&result),
                "md" | "markdown" => report::render_markdown(&result),
                other => {
                    return Err(format!(
                        "Unknown format \"{other}\". Use \"md\" for a readable document or \
                         \"json\" for data."
                    ));
                }
            });
        }
        Command::Tui | Command::GenMan { .. } => {
            unreachable!("handled before reaching the non-interactive commands")
        }
    }
    Ok(())
}

/// Doctor prints its findings, then *offers* the setup steps.
///
/// Setup guidance is opt-in by design: the engine only ever produces guides,
/// and nothing is shown until the user says yes. When stdout is not a terminal
/// — piped into a file, or run from a script — the question cannot be answered,
/// so the answer is no.
fn doctor(json: bool) -> Result<(), String> {
    let result = report::gather(&RealSys);

    if json {
        print(&report::render_json(&result));
        return Ok(());
    }

    print(&report::render_doctor(&result));

    if !has_fixes(&result) {
        return Ok(());
    }
    if !std::io::stdout().is_terminal() {
        println!("Run windrose doctor in a terminal to see step-by-step setup instructions.");
        return Ok(());
    }
    if !ask("Show setup steps for the items above?") {
        return Ok(());
    }

    print(&report::render_fixes(&result.health));
    print(&report::render_fixes(&result.perf));
    Ok(())
}

fn has_fixes(result: &ScanResult) -> bool {
    result
        .health
        .iter()
        .chain(&result.perf)
        .any(|c| c.fix.is_some())
}

/// Ask a yes/no question, defaulting to no. Anything but an explicit yes — an
/// empty line, end of input, an unreadable answer — means no.
fn ask(question: &str) -> bool {
    print!("\n{question} [y/N] ");
    if std::io::stdout().flush().is_err() {
        return false;
    }

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn print(text: &str) {
    println!("{}", text.trim_end());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_means_tui() {
        let cli = Cli::try_parse_from(["windrose"]).unwrap();
        assert!(cli.command.is_none());
    }
}
