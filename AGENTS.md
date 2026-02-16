# Repository Guidelines

## Project Structure & Module Organization
Rust code lives in `src/`, with `main.rs` wiring the primary modules. `src/app` orchestrates runtime state and navigation, `src/ui` holds Ratatui views, and `runtime.rs` drives Tokio tasks. API access sits in `api.rs`, profile data lives in `profile*.rs`, while detection logic spans `detect.rs` and `replay*.rs`. Assets ship beside the binary (for example `player_list.json` from older builds), and runtime logs write to `logs/`. Packaging lives in `flake.nix` (Nix bundle), `install-bwtools.sh` (local install), and `.github/workflows/build.yml` (CI builds and artifacts).

## Build, Test, and Development Commands
- `cargo check` — quick validation during iteration.
- `cargo test` — execute unit tests; add `-- --nocapture` to inspect output.
- `cargo clippy --all-targets --all-features -D warnings` — lint gate used before review.
- `cargo fmt` — run Rustfmt (4-space indent, trailing commas on multi-line structs) prior to commits.

## Coding Style & Naming Conventions
Follow Rust 2024 defaults: modules/files use `snake_case`, types `UpperCamelCase`, and functions descriptive verbs (`load_profile`, `poll_cache`). Keep `use crate::...` imports grouped at the top, prefer explicit structs over tuple returns, and wire new UI widgets through `src/ui/mod.rs`. Emit telemetry with `tracing::{info,warn,error}!` rather than `println!`, and centralize filesystem paths via `config.rs` helpers.
