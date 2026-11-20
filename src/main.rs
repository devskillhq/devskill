mod catalog;
mod cli;
mod installer;
mod ui;

use anyhow::Result;
use clap::Parser;

use crate::catalog::Catalog;
use crate::cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let catalog = Catalog::load(&cli.catalog)?;

    match cli.command {
        Some(Command::List) => ui::print_catalog(&catalog),
        Some(Command::Install { names }) => {
            let items = catalog.find_all(&names)?;
            installer::install(&items, &cli.project)?;
        }
        None => {
            let items = ui::select_items(&catalog)?;
            if items.is_empty() {
                println!("Nothing selected.");
                return Ok(());
            }
            installer::install(&items, &cli.project)?;
        }
    }
    Ok(())
}
