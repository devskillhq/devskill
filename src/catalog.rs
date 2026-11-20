use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A directory containing `SKILL.md` plus any supporting files.
    Skill,
    /// A single markdown file.
    Agent,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Skill => write!(f, "skill"),
            Kind::Agent => write!(f, "agent"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub description: String,
    /// Skill directory or agent `.md` file inside the catalog.
    pub source: PathBuf,
    /// `source` relative to the catalog root, with `/` separators (recorded in the lock file).
    pub path: String,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub items: Vec<Item>,
}

impl Catalog {
    /// Layout: `<root>/skills/<name>/SKILL.md` and `<root>/agents/<name>.md`.
    pub fn load(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            bail!("catalog directory not found: {}", root.display());
        }
        let mut items = Vec::new();

        let skills = root.join("skills");
        if skills.is_dir() {
            for entry in sorted_entries(&skills)? {
                let manifest = entry.join("SKILL.md");
                if entry.is_dir() && manifest.is_file() {
                    items.push(read_item(root, Kind::Skill, &manifest, entry.clone())?);
                }
            }
        }

        let agents = root.join("agents");
        if agents.is_dir() {
            for entry in sorted_entries(&agents)? {
                if entry.extension().is_some_and(|e| e == "md") {
                    items.push(read_item(root, Kind::Agent, &entry, entry.clone())?);
                }
            }
        }

        Ok(Self { items })
    }

    pub fn find_by_path(&self, path: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.path == path)
    }

    pub fn find_all(&self, names: &[String]) -> Result<Vec<Item>> {
        names
            .iter()
            .map(|n| {
                self.items
                    .iter()
                    .find(|i| &i.name == n)
                    .cloned()
                    .with_context(|| format!("no skill or agent named `{n}` in catalog"))
            })
            .collect()
    }
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    Ok(paths)
}

fn read_item(root: &Path, kind: Kind, manifest: &Path, source: PathBuf) -> Result<Item> {
    let text =
        fs::read_to_string(manifest).with_context(|| format!("reading {}", manifest.display()))?;
    let fm = Frontmatter::parse(&text);
    let fallback = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = source
        .strip_prefix(root)
        .unwrap_or(&source)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok(Item {
        kind,
        name: fm.name.unwrap_or(fallback),
        description: fm.description.unwrap_or_default(),
        source,
        path,
    })
}

/// Minimal YAML frontmatter reader: only single-line `name:` / `description:` keys.
#[derive(Debug, Default, PartialEq)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

impl Frontmatter {
    fn parse(text: &str) -> Self {
        let mut fm = Self::default();
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some("---") {
            return fm;
        }
        for line in lines {
            if line.trim() == "---" {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim().trim_matches(['"', '\'']).to_string();
                match key.trim() {
                    "name" => fm.name = Some(value),
                    "description" => fm.description = Some(value),
                    _ => {}
                }
            }
        }
        fm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter() {
        let fm = Frontmatter::parse("---\nname: foo\ndescription: \"Does foo\"\n---\nbody");
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description.as_deref(), Some("Does foo"));
    }

    #[test]
    fn missing_frontmatter_is_empty() {
        assert_eq!(
            Frontmatter::parse("# just markdown"),
            Frontmatter::default()
        );
    }
}
