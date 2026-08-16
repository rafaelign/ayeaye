#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if ! command -v cargo-deb >/dev/null 2>&1; then
    echo "cargo-deb not found. Install it with: cargo install cargo-deb --locked" >&2
    exit 1
fi

cd "$REPO_ROOT"
cargo deb -p app "$@"
