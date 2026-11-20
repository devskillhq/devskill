use anyhow::Result;
use dialoguer::{MultiSelect, theme::ColorfulTheme};

use crate::catalog::{Catalog, Item};

pub fn print_catalog(catalog: &Catalog) {
    if catalog.items.is_empty() {
        println!("Catalog is empty.");
        return;
    }
    for item in &catalog.items {
        println!("{:<6} {:<24} {}", item.kind, item.name, item.description);
    }
}

pub fn select_items(catalog: &Catalog) -> Result<Vec<Item>> {
    if catalog.items.is_empty() {
        return Ok(Vec::new());
    }
    let labels: Vec<String> = catalog
        .items
        .iter()
        .map(|i| format!("[{}] {} — {}", i.kind, i.name, i.description))
        .collect();

    let chosen = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select skills and agents to install (space to toggle, enter to confirm)")
        .items(&labels)
        .interact()?;

    Ok(chosen
        .into_iter()
        .map(|i| catalog.items[i].clone())
        .collect())
}
