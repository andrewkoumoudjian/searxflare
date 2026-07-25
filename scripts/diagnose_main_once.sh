#!/usr/bin/env bash
set -u

log=main-diagnostics.log
: > "$log"
overall=0

run_check() {
  local name="$1"
  shift
  echo "===== ${name} =====" | tee -a "$log"
  set +e
  "$@" 2>&1 | tee -a "$log"
  local status=${PIPESTATUS[0]}
  set -e
  echo "===== ${name}: ${status} =====" | tee -a "$log"
  if [[ $status -ne 0 ]]; then
    overall=1
  fi
}

set -e
run_check formatting cargo fmt --check
run_check clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
run_check rust-tests cargo test --workspace
run_check wasm-build cargo build --workspace --target wasm32-unknown-unknown
run_check specs npm run validate:specs
run_check workerd npm run test:worker
run_check bundle npm run bundle
if [[ -d dist ]]; then
  run_check bundle-size npm run bundle:size
fi

exit "$overall"
