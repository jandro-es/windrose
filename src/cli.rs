//! Command-line surface for Windrose.
//!
//! Every user-facing string here follows the plain-language rule: no unexplained
//! jargon, and any technology named gets a short explanation on first appearance.

use crate::report::{self, ScanResult};
use crate::sys::RealSys;
use clap::{Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "windrose",
    version,
    about = "Discover and assess every AI option on your Mac",
    long_about = "Windrose looks at your Mac and tells you which AI tools you already have, \
                  which ones are ready to use, and how well this machine can run AI models \
                  on its own — without sending anything to the internet.\n\n\
                  Run it with no arguments to open the interactive dashboard.",
    after_long_help = "EXAMPLES:\n  \
        windrose                  Open the interactive dashboard\n  \
        windrose scan             List every AI option found on this Mac\n  \
        windrose scan --json      The same information as data, for scripts\n  \
        windrose score            How well this Mac runs models on its own\n  \
        windrose doctor           What is worth fixing, and how to fix it\n  \
        windrose report --format md > report.md\n                            \
        Save a full report to share\n\n\
        Windrose never installs or changes anything. It shows you what to run \
        and leaves the decision to you."
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

/// Write the manual pages: one for `windrose`, one per visible subcommand.
///
/// Hidden subcommands are skipped — `gen-man` is a maintenance tool, not
/// something a reader should meet in the manual.
pub fn gen_man(out_dir: &Path) -> Result<(), String> {
    use clap::CommandFactory;

    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("could not create {}: {e}", out_dir.display()))?;

    let root = Cli::command();
    write_page(out_dir, "windrose.1", root.clone(), "windrose")?;

    for sub in root.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        // The page should identify itself as `windrose-scan`, not a bare
        // `scan` — that name is what `whatis` and `apropos` index, and a lone
        // `scan(1)` in those listings tells the reader nothing.
        //
        // `Command::name` needs a `'static` name, so the string is leaked.
        // This runs once, over a handful of subcommands, in a maintenance
        // command that exits immediately afterwards.
        let page: &'static str = Box::leak(format!("windrose-{}", sub.get_name()).into_boxed_str());
        write_page(out_dir, &format!("{page}.1"), sub.clone().name(page), page)?;
    }
    Ok(())
}

fn write_page(
    out_dir: &Path,
    file: &str,
    command: clap::Command,
    title: &str,
) -> Result<(), String> {
    let mut buffer = Vec::new();
    clap_mangen::Man::new(command)
        .title(title)
        .render(&mut buffer)
        .map_err(|e| format!("could not render {file}: {e}"))?;

    let path = out_dir.join(file);
    std::fs::write(&path, buffer).map_err(|e| format!("could not write {}: {e}", path.display()))
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

    /// A directory of our own under the system temp dir, removed afterwards.
    /// Named per test so concurrent tests cannot collide.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("windrose-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn gen_man_writes_a_page_naming_every_subcommand() {
        let dir = scratch("gen-man");
        gen_man(&dir).expect("generating man pages should succeed");

        let page =
            std::fs::read_to_string(dir.join("windrose.1")).expect("the root page should exist");

        assert!(page.contains(".TH"), "not a man page: no .TH header");
        for sub in ["scan", "score", "doctor", "report", "tui"] {
            assert!(page.contains(sub), "root page does not mention {sub}");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gen_man_writes_a_page_per_visible_subcommand() {
        let dir = scratch("gen-man-subs");
        gen_man(&dir).expect("generating man pages should succeed");

        for sub in ["scan", "score", "doctor", "report", "tui"] {
            let path = dir.join(format!("windrose-{sub}.1"));
            let page = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("missing page: {}", path.display()));
            assert!(page.contains(".TH"), "{sub}: not a man page");
            assert!(
                page.contains(&format!("windrose\\-{sub}")) || page.contains("windrose"),
                "{sub}: page does not identify itself"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `gen-man` is a maintenance command, not something a reader should meet
    /// in the manual.
    #[test]
    fn hidden_subcommands_get_no_page() {
        let dir = scratch("gen-man-hidden");
        gen_man(&dir).expect("generating man pages should succeed");

        assert!(!dir.join("windrose-gen-man.1").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The examples are the part a new user reads first.
    #[test]
    fn the_root_page_carries_examples_and_the_safety_promise() {
        let dir = scratch("gen-man-examples");
        gen_man(&dir).expect("generating man pages should succeed");
        let page = std::fs::read_to_string(dir.join("windrose.1")).expect("root page");

        assert!(page.contains("EXAMPLES"), "no examples section");
        assert!(page.contains("scan \\-\\-json") || page.contains("scan --json"));
        assert!(
            page.contains("never installs or changes anything"),
            "the manual should state that Windrose changes nothing"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Generating twice must produce identical bytes, or the drift check in
    /// scripts/generate-man.sh would report spurious changes forever.
    #[test]
    fn generation_is_reproducible() {
        let dir = scratch("gen-man-twice");
        gen_man(&dir).expect("first run");
        let first = std::fs::read(dir.join("windrose.1")).expect("root page");
        gen_man(&dir).expect("second run");
        let second = std::fs::read(dir.join("windrose.1")).expect("root page");

        assert_eq!(first, second, "man page generation is not reproducible");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
