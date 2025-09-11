#!/usr/bin/env bash
set -euo pipefail

echo "[linux] building release..."
cargo build --release

echo "[windows] building release via nix devShell..."
nix develop .#windows -c cargo build --release

LINUX_BIN="target/release/bwtools"
WIN_BIN="target/x86_64-pc-windows-gnu/release/bwtools.exe"

echo "\nBuild artifacts:"
printf " - %s %s\n" "$LINUX_BIN" "$( [ -f "$LINUX_BIN" ] && echo OK || echo MISSING )"
printf " - %s %s\n" "$WIN_BIN" "$( [ -f "$WIN_BIN" ] && echo OK || echo MISSING )"

echo "Done."

