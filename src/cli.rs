use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_CATALOG: &str = "https://github.com/devskillhq/devskill.git";

/// Install AI skills and agents into a project.
///
/// Run without a subcommand to pick items interactively.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Catalog to install from: a local directory or a git URL, optionally
    /// pinned with `#<branch-or-tag>` (e.g. `https://github.com/org/skills.git#v1`).
    /// The catalog has `skills/` and `agents/` subfolders.
    #[arg(
        long,
        global = true,
        env = "DEVSKILL_CATALOG",
        default_value = DEFAULT_CATALOG
    )]
    pub catalog: String,

    /// Use the cached copy of a git catalog without fetching updates.
    #[arg(long, global = true)]
    pub offline: bool,

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
    /// Install items by name without prompting. With no names, restore everything
    /// recorded in the project's skills-lock.json at the locked commits.
    Install { names: Vec<String> },
    /// Show locked items and whether the installed files match the lock.
    Status,
}
