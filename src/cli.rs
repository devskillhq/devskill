use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Install AI skills and agents into a project.
///
/// Run without a subcommand to pick items interactively.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Directory containing the catalog (`skills/` and `agents/` subfolders).
    #[arg(
        long,
        global = true,
        env = "DEVSKILL_CATALOG",
        default_value = "catalog"
    )]
    pub catalog: PathBuf,

    /// Project root to install into.
    #[arg(long, short, global = true, default_value = ".")]
    pub project: PathBuf,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List available skills and agents.
    List,
    /// Install items by name without prompting.
    Install {
        #[arg(required = true)]
        names: Vec<String>,
    },
}
