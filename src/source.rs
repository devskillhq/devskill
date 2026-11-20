//! Resolves a `--catalog` spec to a local directory, cloning git catalogs into a cache.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// A git catalog: repository URL plus optional branch/tag.
#[derive(Debug, PartialEq, Eq)]
pub struct GitSpec {
    pub url: String,
    pub reference: Option<String>,
}

impl GitSpec {
    /// Parses `<url>[#<ref>]`. Returns `None` if the spec isn't a git URL.
    pub fn parse(spec: &str) -> Option<Self> {
        let (url, reference) = match spec.rsplit_once('#') {
            Some((u, r)) if !r.is_empty() => (u, Some(r.to_string())),
            _ => (spec, None),
        };
        let is_git = ["https://", "http://", "ssh://", "git://", "file://", "git@"]
            .iter()
            .any(|p| url.starts_with(p))
            || url.ends_with(".git");
        is_git.then(|| Self {
            url: url.to_string(),
            reference,
        })
    }

    /// Stable, filesystem-safe cache directory name for this spec.
    fn cache_key(&self) -> String {
        let raw = match &self.reference {
            Some(r) => format!("{}#{}", self.url, r),
            None => self.url.clone(),
        };
        raw.trim_start_matches("https://")
            .trim_start_matches("ssh://")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}

/// A catalog resolved to a local directory, plus where it came from (for the lock file).
#[derive(Debug)]
pub struct Resolved {
    pub root: PathBuf,
    /// Git URL, or canonical local path.
    pub source: String,
    pub reference: Option<String>,
    /// Commit checked out in `root`, if it is a git work tree.
    pub commit: Option<String>,
    /// True if `root` is our cache clone (safe to check out other commits in).
    pub cached: bool,
}

/// Resolves a catalog spec to a local directory.
///
/// Existing local paths win; otherwise git URLs are cloned (or refreshed) under the
/// user cache directory. With `offline`, an existing clone is used as-is.
pub fn resolve(spec: &str, offline: bool) -> Result<Resolved> {
    if Path::new(spec).exists() {
        let root = Path::new(spec)
            .canonicalize()
            .with_context(|| format!("resolving {spec}"))?;
        return Ok(Resolved {
            source: root.to_string_lossy().into_owned(),
            commit: head_commit(&root),
            root,
            reference: None,
            cached: false,
        });
    }
    match GitSpec::parse(spec) {
        Some(git) => {
            let root = sync(&git, &cache_root()?, offline)?;
            Ok(Resolved {
                commit: head_commit(&root),
                root,
                source: git.url,
                reference: git.reference,
                cached: true,
            })
        }
        None => bail!("catalog not found: `{spec}` is neither a directory nor a git URL"),
    }
}

/// Moves a cached clone to `commit` (fetching it if needed).
pub fn checkout(resolved: &mut Resolved, commit: &str) -> Result<()> {
    if resolved.commit.as_deref() == Some(commit) {
        return Ok(());
    }
    if !resolved.cached {
        bail!(
            "{} is at {}, lock expects {commit}; check it out manually",
            resolved.source,
            resolved.commit.as_deref().unwrap_or("an unknown commit")
        );
    }
    run_git(
        &resolved.root,
        &["fetch", "-q", "--depth", "1", "origin", commit],
    )
    .and_then(|()| run_git(&resolved.root, &["reset", "-q", "--hard", "FETCH_HEAD"]))
    .with_context(|| format!("checking out {commit} from {}", resolved.source))?;
    resolved.commit = Some(commit.to_string());
    Ok(())
}

fn head_commit(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn cache_root() -> Result<PathBuf> {
    let base = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .context("cannot determine a cache directory (set XDG_CACHE_HOME)")?;
    Ok(base.join("devskill").join("catalogs"))
}

/// Clones `git` into `cache_root`, or fast-forwards an existing clone to the latest
/// commit of the requested ref. Falls back to the cached copy if fetching fails.
pub fn sync(git: &GitSpec, cache_root: &Path, offline: bool) -> Result<PathBuf> {
    let dir = cache_root.join(git.cache_key());

    if dir.join(".git").is_dir() {
        if offline {
            return Ok(dir);
        }
        let refspec = git.reference.as_deref().unwrap_or("HEAD");
        let fetched = run_git(&dir, &["fetch", "-q", "--depth", "1", "origin", refspec])
            .and_then(|()| run_git(&dir, &["reset", "-q", "--hard", "FETCH_HEAD"]));
        if let Err(e) = fetched {
            eprintln!(
                "warning: could not update {} ({e:#}); using cached copy",
                git.url
            );
        }
        return Ok(dir);
    }

    if offline {
        bail!("{} is not cached yet; run once without --offline", git.url);
    }
    std::fs::create_dir_all(cache_root)
        .with_context(|| format!("creating {}", cache_root.display()))?;
    let mut args = vec!["clone", "-q", "--depth", "1"];
    if let Some(r) = &git.reference {
        args.extend(["--branch", r]);
    }
    let dir_str = dir.to_string_lossy();
    args.extend([git.url.as_str(), dir_str.as_ref()]);
    run_git(cache_root, &args).with_context(|| format!("cloning {}", git.url))?;
    Ok(dir)
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .context("failed to run `git`; is it installed?")?;
    if !status.success() {
        bail!("`git {}` failed ({status})", args.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_git_specs() {
        assert_eq!(
            GitSpec::parse("https://github.com/o/r.git#v1"),
            Some(GitSpec {
                url: "https://github.com/o/r.git".into(),
                reference: Some("v1".into())
            })
        );
        assert!(GitSpec::parse("git@github.com:o/r.git").is_some());
        assert!(GitSpec::parse("https://github.com/o/r").is_some());
        assert_eq!(GitSpec::parse("catalog"), None);
        assert_eq!(GitSpec::parse("./some/dir"), None);
    }

    #[test]
    fn cache_key_is_filesystem_safe() {
        let key = GitSpec::parse("git@github.com:o/r.git#main")
            .unwrap()
            .cache_key();
        assert_eq!(key, "git_github.com_o_r.git_main");
    }

    fn git(cwd: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn clones_then_updates_from_remote() -> Result<()> {
        let remote = tempfile::tempdir()?;
        let cache = tempfile::tempdir()?;
        let r = remote.path();
        git(r, &["init", "-q"]);
        fs::create_dir_all(r.join("skills/one"))?;
        fs::write(r.join("skills/one/SKILL.md"), "v1")?;
        git(r, &["add", "."]);
        git(r, &["commit", "-qm", "v1"]);

        let spec = GitSpec {
            url: format!("file://{}", r.display()),
            reference: None,
        };
        let dir = sync(&spec, cache.path(), false)?;
        assert_eq!(fs::read_to_string(dir.join("skills/one/SKILL.md"))?, "v1");

        fs::write(r.join("skills/one/SKILL.md"), "v2")?;
        git(r, &["commit", "-qam", "v2"]);

        // Offline keeps the cached copy; a normal run picks up the new commit.
        sync(&spec, cache.path(), true)?;
        assert_eq!(fs::read_to_string(dir.join("skills/one/SKILL.md"))?, "v1");
        sync(&spec, cache.path(), false)?;
        assert_eq!(fs::read_to_string(dir.join("skills/one/SKILL.md"))?, "v2");
        Ok(())
    }
}
