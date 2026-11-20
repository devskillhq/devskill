# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`devskill` is a Rust CLI that installs AI skills and agents into a project. The user runs it, picks items from a catalog (interactive multi-select), and the tool copies them into the project's `.claude/` directory.

## Commands

Rust is provided by `mise`, pinned in `mise.toml`. Inside the repo the mise shims resolve `cargo` directly; in non-interactive shells where shims aren't active, use `mise exec -- cargo ...`.

```sh
cargo build
cargo test                          # all tests
cargo test parses_frontmatter       # single test by name
cargo clippy --all-targets
cargo fmt
cargo run                           # interactive picker (needs a TTY)
cargo run -- list
cargo run -- install <name>... -p <project-dir>
```

Global flags: `--catalog <dir>` (env `DEVSKILL_CATALOG`, default `./catalog`) and `--project/-p <dir>` (default `.`).

## Architecture

The flow is linear: `cli` parses args → `catalog` discovers items → `ui` (interactive) or `Catalog::find_all` (by name) picks a subset → `installer` copies them.

- **Catalog layout** (`src/catalog.rs`): `<catalog>/skills/<name>/SKILL.md` (a skill is a whole directory, copied recursively) and `<catalog>/agents/<name>.md` (an agent is a single file). `name`/`description` come from YAML frontmatter; the parser is deliberately minimal (single-line `key: value` only, no YAML crate). If `name` is missing, the directory/file stem is used.
- **Install targets** (`src/installer.rs`): skills are copied to the tool-agnostic `<project>/.agents/skills/<name>/`, and `<project>/.claude/skills/<name>` is a per-skill **relative** symlink (`../../.agents/skills/<name>`) pointing at it. Agents → `<project>/.claude/agents/<name>.md`. Existing files are overwritten and existing symlinks replaced, but a real file/dir at the symlink path is an error (never deleted). New per-tool links for other agent tools should follow the same `.agents` + symlink pattern.
- `Kind` (Skill/Agent) is the switch that drives both discovery and install behavior; adding a new item type means extending `Kind`, `Catalog::load`, and `installer`.
- `catalog/` in the repo is a sample catalog with placeholder items used for manual testing.
