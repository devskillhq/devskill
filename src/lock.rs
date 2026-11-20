//! `skills-lock.json`: records what was installed into a project and from where, so
//! installs can be restored on another machine and local edits detected.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalog::{Item, Kind};
use crate::installer;
use crate::source::Resolved;

pub const FILE_NAME: &str = "skills-lock.json";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default)]
    pub skills: BTreeMap<String, Entry>,
    #[serde(default)]
    pub agents: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Git URL or local path of the catalog.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Item path inside the catalog, e.g. `skills/pest-testing`.
    pub path: String,
    /// SHA-256 of the installed files (see [`hash_path`]).
    pub computed_hash: String,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: VERSION,
            skills: BTreeMap::new(),
            agents: BTreeMap::new(),
        }
    }
}

impl Lockfile {
    pub fn path(project: &Path) -> PathBuf {
        project.join(FILE_NAME)
    }

    /// Loads the project's lock file, or an empty one if it doesn't exist.
    pub fn load(project: &Path) -> Result<Self> {
        let path = Self::path(project);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, project: &Path) -> Result<()> {
        let path = Self::path(project);
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Records freshly installed `items` (hashing what landed in the project).
    pub fn record(&mut self, items: &[Item], origin: &Resolved, project: &Path) -> Result<()> {
        for item in items {
            let entry = Entry {
                source: origin.source.clone(),
                reference: origin.reference.clone(),
                commit: origin.commit.clone(),
                path: item.path.clone(),
                computed_hash: hash_path(&installer::destination(item, project))?,
            };
            let section = match item.kind {
                Kind::Skill => &mut self.skills,
                Kind::Agent => &mut self.agents,
            };
            section.insert(item.name.clone(), entry);
        }
        Ok(())
    }

    /// All entries as `(kind, name, entry)`.
    pub fn entries(&self) -> impl Iterator<Item = (Kind, &String, &Entry)> {
        self.skills
            .iter()
            .map(|(n, e)| (Kind::Skill, n, e))
            .chain(self.agents.iter().map(|(n, e)| (Kind::Agent, n, e)))
    }
}

/// SHA-256 over a file or directory tree: for each file in sorted relative-path order,
/// `path \0 contents \0`. `.git` is skipped; symlinks are followed.
pub fn hash_path(path: &Path) -> Result<String> {
    let mut files = Vec::new();
    if path.is_dir() {
        collect_files(path, path, &mut files)?;
    } else {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        files.push((name, path.to_path_buf()));
    }
    files.sort();

    let mut hasher = Sha256::new();
    for (rel, abs) in files {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(&abs).with_context(|| format!("reading {}", abs.display()))?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        if path.is_dir() {
            collect_files(base, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(base)?
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, path));
        }
    }
    Ok(())
}

/// Install state of a locked item compared with the project's files.
#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Modified,
    Missing,
}

pub fn status(kind: Kind, name: &str, entry: &Entry, project: &Path) -> Result<Status> {
    let dest = installer::destination_for(kind, name, project);
    if !dest.exists() {
        return Ok(Status::Missing);
    }
    Ok(if hash_path(&dest)? == entry.computed_hash {
        Status::Ok
    } else {
        Status::Modified
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_content_sensitive() -> Result<()> {
        let a = tempfile::tempdir()?;
        fs::create_dir_all(a.path().join("sub"))?;
        fs::write(a.path().join("SKILL.md"), "x")?;
        fs::write(a.path().join("sub/ref.md"), "y")?;
        let h1 = hash_path(a.path())?;
        assert_eq!(h1, hash_path(a.path())?);

        fs::write(a.path().join("sub/ref.md"), "z")?;
        assert_ne!(h1, hash_path(a.path())?);
        Ok(())
    }

    #[test]
    fn round_trips_through_json() -> Result<()> {
        let project = tempfile::tempdir()?;
        let mut lock = Lockfile::default();
        lock.skills.insert(
            "s".into(),
            Entry {
                source: "https://example.com/r.git".into(),
                reference: None,
                commit: Some("abc".into()),
                path: "skills/s".into(),
                computed_hash: "h".into(),
            },
        );
        lock.save(project.path())?;
        let text = fs::read_to_string(Lockfile::path(project.path()))?;
        assert!(text.contains("\"computedHash\": \"h\""));
        assert!(!text.contains("reference"));
        assert_eq!(Lockfile::load(project.path())?, lock);
        Ok(())
    }
}
