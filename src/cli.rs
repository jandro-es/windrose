//! Command-line surface for Windrose.
//!
//! Every user-facing string here follows the plain-language rule: no unexplained
//! jargon, and any technology named gets a short explanation on first appearance.

use clap::{Parser, Subcommand};

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
