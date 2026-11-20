use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::catalog::{Item, Kind};

/// Where an item's files land inside the target project.
///
/// Skills live in the tool-agnostic `.agents/skills/` directory; agents go to `.claude/agents/`.
pub fn destination(item: &Item, project: &Path) -> PathBuf {
    match item.kind {
        Kind::Skill => project.join(".agents/skills").join(&item.name),
        Kind::Agent => project
            .join(".claude/agents")
            .join(format!("{}.md", item.name)),
    }
}

/// Path of the `.claude/skills/<name>` symlink that points at the installed skill.
pub fn skill_link(name: &str, project: &Path) -> PathBuf {
    project.join(".claude/skills").join(name)
}

pub fn install(items: &[Item], project: &Path) -> Result<()> {
    for item in items {
        let dest = destination(item, project);
        match item.kind {
            Kind::Skill => {
                copy_dir(&item.source, &dest)?;
                let link = skill_link(&item.name, project);
                // Relative target so the link survives moving/cloning the project.
                let target = Path::new("../../.agents/skills").join(&item.name);
                replace_symlink(&target, &link)?;
                println!(
                    "installed skill {} -> {} (linked from {})",
                    item.name,
                    dest.display(),
                    link.display()
                );
            }
            Kind::Agent => {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&item.source, &dest)
                    .with_context(|| format!("copying {}", item.source.display()))?;
                println!("installed agent {} -> {}", item.name, dest.display());
            }
        }
    }
    Ok(())
}

/// Create `link -> target`, replacing an existing symlink. Refuses to touch a real file/dir.
fn replace_symlink(target: &Path, link: &Path) -> Result<()> {
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            remove_symlink(link).with_context(|| format!("removing {}", link.display()))?
        }
        Ok(_) => bail!(
            "{} already exists and is not a symlink; remove it and re-run",
            link.display()
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("inspecting {}", link.display())),
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    symlink_dir(target, link).with_context(|| format!("linking {}", link.display()))
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(unix)]
fn remove_symlink(link: &Path) -> io::Result<()> {
    fs::remove_file(link)
}

#[cfg(windows)]
fn remove_symlink(link: &Path) -> io::Result<()> {
    fs::remove_dir(link)
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures(catalog: &Path) -> Result<Vec<Item>> {
        let skill_src = catalog.join("s");
        fs::create_dir_all(&skill_src)?;
        fs::write(skill_src.join("SKILL.md"), "x")?;
        let agent_src = catalog.join("a.md");
        fs::write(&agent_src, "y")?;
        Ok(vec![
            Item {
                kind: Kind::Skill,
                name: "s".into(),
                description: String::new(),
                source: skill_src,
            },
            Item {
                kind: Kind::Agent,
                name: "a".into(),
                description: String::new(),
                source: agent_src,
            },
        ])
    }

    #[test]
    fn installs_skill_into_agents_with_claude_symlink() -> Result<()> {
        let catalog = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let items = fixtures(catalog.path())?;

        install(&items, project.path())?;
        // Re-installing must replace the existing symlink cleanly.
        install(&items, project.path())?;

        let p = project.path();
        assert!(p.join(".agents/skills/s/SKILL.md").is_file());
        let link = p.join(".claude/skills/s");
        assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(fs::read_link(&link)?, Path::new("../../.agents/skills/s"));
        assert!(link.join("SKILL.md").is_file());
        assert!(p.join(".claude/agents/a.md").is_file());
        Ok(())
    }

    #[test]
    fn refuses_to_replace_real_directory() -> Result<()> {
        let catalog = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let items = fixtures(catalog.path())?;
        fs::create_dir_all(project.path().join(".claude/skills/s"))?;

        assert!(install(&items[..1], project.path()).is_err());
        Ok(())
    }
}
