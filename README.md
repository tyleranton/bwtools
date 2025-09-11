# bwtools

A terminal UI tool for StarCraft: Remastered that detects the local web API, identifies your current profile and opponent, and surfaces useful info like ratings and opponent toons. Built with Ratatui and integrates the `bw-web-api-rs` client.

## Features
- Detects your profile and opponent when loading into a game.
- Shows your region and current rating; polls the API every 60s to keep rating fresh.
- Lists opponent toons with gateway/region and highest observed rating per toon.

## Requirements
- Rust (stable). Tokio is used internally by the API client.

## Build

### NixOS (recommended)
- Dev shell for native Linux build:
  - `nix develop -c cargo build --release`
- Dev shell for Windows cross‑build:
  - `nix develop .#windows -c cargo build --release`
- Convenience script (builds both):
  - `./scripts/build-all.sh`
- Artifacts:
  - Linux: `target/release/bwtools`
  - Windows: `target/x86_64-pc-windows-gnu/release/bwtools.exe`

### Non‑Nix (Linux)
- Install Rust and Cargo.
- `cargo build --release`

### Non‑Nix (cross‑compile to Windows)
- Install MinGW toolchain and target:
  - `rustup target add x86_64-pc-windows-gnu`
  - Install `mingw-w64` (package varies by distro)
- Build:
  - `cargo build --release --target x86_64-pc-windows-gnu`

## Run
- `cargo run`

## Cache Directory
- Windows: `%USERPROFILE%/AppData/Local/Temp/blizzard_browser_cache`
- Linux (Wine): `$HOME/.wine-battlenet/drive_c/users/$USER/AppData/Local/Temp/blizzard_browser_cache`

## Project Layout
- `src/app.rs` — App state and key handling
- `src/ui.rs` — TUI rendering (Status/Main/Debug)
- `src/tui.rs` — Terminal setup/teardown (crossterm + Ratatui backend)
- `src/cache.rs` — Chrome cache scanning and URL parsing
- `src/api.rs` — `bw-web-api-rs` client wrapper and helpers
- `src/config.rs` — runtime config (tick rates, windows, cache dir)
