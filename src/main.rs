mod catalog;
mod cli;
mod installer;
mod lock;
mod source;
mod ui;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::catalog::{Catalog, Item};
use crate::cli::{Cli, Command};
use crate::lock::{Lockfile, Status};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project = cli.project.as_path();

    match cli.command {
        Some(Command::List) => {
            let (_, catalog) = load_catalog(&cli.catalog, cli.offline)?;
            ui::print_catalog(&catalog);
        }
        Some(Command::Status) => status(project)?,
        Some(Command::Install { names }) if names.is_empty() => restore(project, cli.offline)?,
        Some(Command::Install { names }) => {
            let (origin, catalog) = load_catalog(&cli.catalog, cli.offline)?;
            install_and_lock(&catalog.find_all(&names)?, &origin, project)?;
        }
        None => {
            let (origin, catalog) = load_catalog(&cli.catalog, cli.offline)?;
            let items = ui::select_items(&catalog)?;
            if items.is_empty() {
                println!("Nothing selected.");
                return Ok(());
            }
            install_and_lock(&items, &origin, project)?;
        }
    }
    Ok(())
}

fn load_catalog(spec: &str, offline: bool) -> Result<(source::Resolved, Catalog)> {
    let origin = source::resolve(spec, offline)?;
    let catalog = Catalog::load(&origin.root)?;
    Ok((origin, catalog))
}

fn install_and_lock(items: &[Item], origin: &source::Resolved, project: &Path) -> Result<()> {
    installer::install(items, project)?;
    let mut lock = Lockfile::load(project)?;
    lock.record(items, origin, project)?;
    lock.save(project)?;
    println!("updated {}", lock::FILE_NAME);
    Ok(())
}

fn status(project: &Path) -> Result<()> {
    let lock = Lockfile::load(project)?;
    if lock.entries().next().is_none() {
        println!("No {} in {}.", lock::FILE_NAME, project.display());
        return Ok(());
    }
    let mut dirty = false;
    for (kind, name, entry) in lock.entries() {
        let state = match lock::status(kind, name, entry, project)? {
            Status::Ok => "ok",
            Status::Modified => "modified",
            Status::Missing => "missing",
        };
        dirty |= state != "ok";
        let commit = entry.commit.as_deref().map_or("", |c| &c[..c.len().min(7)]);
        println!("{kind:<6} {name:<24} {state:<9} {commit} {}", entry.source);
    }
    if dirty {
        bail!("installed files differ from {}", lock::FILE_NAME);
    }
    Ok(())
}

/// Reinstalls every locked item from its recorded source and commit.
fn restore(project: &Path, offline: bool) -> Result<()> {
    let lock = Lockfile::load(project)?;
    if lock.entries().next().is_none() {
        bail!(
            "nothing to restore: no entries in {}; pass names to install",
            Lockfile::path(project).display()
        );
    }

    // Group by where items come from, so each source/commit is fetched once.
    // (source, reference, commit) -> entries
    type Origin = (String, Option<String>, Option<String>);
    let mut groups: BTreeMap<Origin, Vec<&lock::Entry>> = BTreeMap::new();
    for (_, _, entry) in lock.entries() {
        let key = (
            entry.source.clone(),
            entry.reference.clone(),
            entry.commit.clone(),
        );
        groups.entry(key).or_default().push(entry);
    }

    for ((source, reference, commit), entries) in groups {
        let spec = match &reference {
            Some(r) => format!("{source}#{r}"),
            None => source.clone(),
        };
        let mut origin = source::resolve(&spec, offline)?;
        if let Some(commit) = &commit {
            source::checkout(&mut origin, commit)?;
        }
        let catalog = Catalog::load(&origin.root)?;
        let items = entries
            .iter()
            .map(|e| {
                catalog
                    .find_by_path(&e.path)
                    .cloned()
                    .with_context(|| format!("`{}` not found in {spec}", e.path))
            })
            .collect::<Result<Vec<_>>>()?;
        installer::install(&items, project)?;
        for (item, entry) in items.iter().zip(&entries) {
            if lock::hash_path(&installer::destination(item, project))? != entry.computed_hash {
                eprintln!(
                    "warning: {} {} does not match the locked hash",
                    item.kind, item.name
                );
            }
        }
    }
    Ok(())
}
