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

rustup component add rustfmt clippy --toolchain "$RUST_TOOLCHAIN"

cargo "+$RUST_TOOLCHAIN" fmt --check
cargo "+$RUST_TOOLCHAIN" clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo "+$RUST_TOOLCHAIN" test --locked --workspace
cargo "+$RUST_TOOLCHAIN" build --locked --workspace --target wasm32-unknown-unknown

SPEC_VALIDATOR_VENV="$ROOT_DIR/target/spec-validator-venv"
python3 -m venv "$SPEC_VALIDATOR_VENV"
"$SPEC_VALIDATOR_VENV/bin/python" -m pip install \
  --disable-pip-version-check \
  jsonschema \
  openapi-spec-validator \
  pyyaml
PATH="$SPEC_VALIDATOR_VENV/bin:$PATH" npm run validate:specs

(
  cd "$ROOT_DIR/crates/metasearch-worker"
  worker-build --release
)

CLOUDFLARE_CF_FETCH_ENABLED=false npx vitest run

rm -rf "$ROOT_DIR/dist"
SEARXFLARE_SKIP_CLOUDFLARE_VALIDATION=1 npx wrangler deploy --dry-run --outdir "$ROOT_DIR/dist"
python3 "$ROOT_DIR/scripts/bundle_size.py" "$ROOT_DIR/dist"
