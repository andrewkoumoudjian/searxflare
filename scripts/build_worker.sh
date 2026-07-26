#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN="$(awk -F '"' '/^channel = / { print $2; exit }' "$ROOT_DIR/rust-toolchain.toml")"
WORKER_BUILD_VERSION="0.8.5"

if [[ -z "$RUST_TOOLCHAIN" ]]; then
  echo "Unable to determine the pinned Rust toolchain" >&2
  exit 1
fi

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "$RUST_TOOLCHAIN"
fi

if [[ -f "$CARGO_HOME/env" ]]; then
  # shellcheck disable=SC1090
  source "$CARGO_HOME/env"
fi

rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --no-self-update
rustup target add wasm32-unknown-unknown --toolchain "$RUST_TOOLCHAIN"

if ! command -v worker-build >/dev/null 2>&1 \
  || ! worker-build --version 2>/dev/null | grep -Fq "$WORKER_BUILD_VERSION"; then
  cargo "+$RUST_TOOLCHAIN" install worker-build --version "$WORKER_BUILD_VERSION" --locked
fi

cd "$ROOT_DIR"
cargo "+$RUST_TOOLCHAIN" metadata --locked --no-deps --format-version 1 >/dev/null

if [[ "${WORKERS_CI:-}" == "1" \
  && "${SEARXFLARE_SKIP_CLOUDFLARE_VALIDATION:-}" != "1" ]]; then
  exec bash "$ROOT_DIR/scripts/cloudflare_validate.sh"
fi

cd "$ROOT_DIR/crates/metasearch-worker"
exec worker-build --release
