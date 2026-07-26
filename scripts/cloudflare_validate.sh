#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN="$(awk -F '"' '/^channel = / { print $2; exit }' "$ROOT_DIR/rust-toolchain.toml")"

if [[ -z "$RUST_TOOLCHAIN" ]]; then
  echo "Unable to determine the pinned Rust toolchain" >&2
  exit 1
fi

if [[ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]]; then
  # shellcheck disable=SC1090
  source "${CARGO_HOME:-$HOME/.cargo}/env"
fi

cd "$ROOT_DIR"

rustup component add rustfmt --toolchain "$RUST_TOOLCHAIN"

# Diagnostic-only: verify the changed engine adapters with the workspace edition.
rustfmt "+$RUST_TOOLCHAIN" --edition 2021 --check \
  crates/metasearch-engines/src/arxiv.rs \
  crates/metasearch-engines/src/wikipedia.rs
cargo "+$RUST_TOOLCHAIN" fmt

(
  cd "$ROOT_DIR/crates/metasearch-worker"
  worker-build --release
)
