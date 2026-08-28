#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

LOCAL_CARGO_HOME="$PROJECT_DIR/.toolchain/cargo"
if [[ -x "$LOCAL_CARGO_HOME/bin/cargo" ]]; then
    export RUSTUP_HOME="$PROJECT_DIR/.toolchain/rustup"
    export CARGO_HOME="$LOCAL_CARGO_HOME"
    export PATH="$CARGO_HOME/bin:$PATH"
elif ! command -v cargo >/dev/null 2>&1; then
    echo "error: Cargo is required; install Rust with https://rustup.rs" >&2
    exit 1
fi

cd "$PROJECT_DIR/codex-rs"
cargo build --locked --release -p codex-cli --bin codex
install -d "$PROJECT_DIR/bin"
install -m 0755 target/release/codex "$PROJECT_DIR/bin/codex-hud"
install -m 0644 "$PROJECT_DIR/codex-hud.toml" "$PROJECT_DIR/bin/codex-hud.toml"
install -m 0644 "$PROJECT_DIR/pricing.toml" "$PROJECT_DIR/bin/pricing.toml"

EXPECTED_VERSION="codex-hud 0.1.0+codex.0.150.1"
ACTUAL_VERSION="$("$PROJECT_DIR/bin/codex-hud" --version)"
if [[ "$ACTUAL_VERSION" != "$EXPECTED_VERSION" ]]; then
    echo "error: expected '$EXPECTED_VERSION', got '$ACTUAL_VERSION'" >&2
    exit 1
fi
echo "$ACTUAL_VERSION"
