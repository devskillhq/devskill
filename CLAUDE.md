# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An AI development stack for Laravel applications, Laravel packages, PHP packages and PHP CLI tools. One repo holds three things:

- **Skills and agents** (`skills/`, `agents/` at the repo root) — the content. This repo *is* the default catalog.
- **Evals** (planned) — run Claude Code headless (`claude -p`) against fixture projects with and without a skill, graded by real checks (Pest/PHPUnit, PHPStan, Pint, file assertions) plus an LLM judge, to measure and improve skills.
- **`devskill` CLI** (Rust, `src/`) — installs skills/agents into a project and records them in `skills-lock.json`. Planned: detect project type from `composer.json`, write agent config (CLAUDE.md/AGENTS.md, `.claude/settings.json`, MCP servers), set up dev tooling (Pest, PHPStan/Larastan, Pint, Rector), and scaffold new projects.

Target agent is Claude Code, but skills install in the open `.agents/skills/` format so other agents that read it also work.

## Commands

Rust is provided by `mise`, pinned in `mise.toml`. Inside the repo the mise shims resolve `cargo` directly; in non-interactive shells where shims aren't active, use `mise exec -- cargo ...`.

```sh
cargo build
cargo test                          # all tests (some shell out to `git`)
cargo test parses_frontmatter       # single test by name
cargo clippy --all-targets
cargo fmt
cargo run -- --catalog . -p <dir>              # interactive picker (needs a TTY)
cargo run -- --catalog . list
cargo run -- --catalog . install <name>... -p <dir>
cargo run -- install -p <dir>                  # restore everything in <dir>/skills-lock.json
cargo run -- status -p <dir>                   # ok/modified/missing; exits 1 if not clean
```

Global flags: `--catalog <dir-or-git-url[#ref]>` (env `DEVSKILL_CATALOG`, default `https://github.com/devskillhq/devskill.git`), `--offline`, `--project/-p <dir>` (default `.`). During development pass `--catalog .` to use the working tree instead of the pushed repo. Don't install into this repo itself — installed `.agents/`/`.claude/` content is meant to be committed in target projects; use `-p` with a scratch dir.

## Architecture

Flow: `cli` parses args → `source` resolves the catalog spec to a local dir (+ provenance) → `catalog` discovers items → `ui` (interactive) or `Catalog::find_all` (by name) picks a subset → `installer` copies them → `lock` records them.

- **Catalog sources** (`src/source.rs`): an existing local path wins; otherwise a git URL (`https://`, `git@`, `ssh://`, `file://`, or `*.git`, optional `#branch-or-tag`) is shallow-cloned by shelling out to the system `git` (so the user's SSH keys/credential helpers apply) into `$XDG_CACHE_HOME/devskill/catalogs/<sanitized-url>`. Later runs `fetch --depth 1` + `reset --hard FETCH_HEAD`; if the fetch fails the cached copy is used with a warning. `--offline` skips fetching. `Resolved` carries the source, ref and HEAD commit for the lock; `checkout` moves only *cache* clones to a locked commit and refuses to touch a local working tree.
- **Catalog layout** (`src/catalog.rs`): `<catalog>/skills/<name>/SKILL.md` (a skill is a whole directory, copied recursively) and `<catalog>/agents/<name>.md` (an agent is a single file). `name`/`description` come from YAML frontmatter; the parser is deliberately minimal (single-line `key: value` only, no YAML crate). If `name` is missing, the directory/file stem is used. `Item.path` is the catalog-relative path stored in the lock.
- **Install targets** (`src/installer.rs`): skills are copied to `<project>/.agents/skills/<name>/`, and `<project>/.claude/skills/<name>` is a per-skill **relative** symlink (`../../.agents/skills/<name>`). Agents → `<project>/.claude/agents/<name>.md`. Existing files are overwritten and existing symlinks replaced, but a real file/dir at the symlink path is an error (never deleted).
- **Lock file** (`src/lock.rs`, `<project>/skills-lock.json`, modelled on `npx skills`): `skills` and `agents` maps keyed by name, each with `source`, optional `reference`, `commit`, catalog `path`, and `computedHash` — SHA-256 over sorted relative file paths and contents (`path \0 contents \0`, `.git` skipped) of the *installed* copy. `install` with no names restores from the lock, grouped by (source, ref, commit), and warns on hash mismatch.
- `Kind` (Skill/Agent) drives discovery, install paths and lock sections; adding a new item type means extending `Kind`, `Catalog::load`, `installer::destination_for` and `Lockfile`.
- `skills/example-skill` and `agents/example-agent.md` are placeholders for manual testing.
