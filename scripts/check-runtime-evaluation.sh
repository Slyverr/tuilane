#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

manifest="experiments/runtime-evaluation/Cargo.toml"
target_dir=${CARGO_TARGET_DIR:-target/runtime-evaluation}
export CARGO_TARGET_DIR="$target_dir"
guest_wasm="$target_dir/wasm32-unknown-unknown/release/tuilane_runtime_evaluation_guest.wasm"
component="$target_dir/runtime-evaluation.component.wasm"

if ! rustup target list --installed --toolchain stable | grep -Fxq wasm32-unknown-unknown; then
  printf '%s\n' 'Missing Rust target wasm32-unknown-unknown.' \
    'Install it with: rustup target add wasm32-unknown-unknown --toolchain stable' >&2
  exit 1
fi

rustup run stable cargo fmt --manifest-path "$manifest" --all -- --check
rustup run stable cargo clippy --locked --manifest-path "$manifest" \
  -p tuilane-runtime-evaluation-host --all-targets -- -D warnings
rustup run stable cargo test --locked --manifest-path "$manifest" -p tuilane-runtime-evaluation-host
rustup run stable cargo build --locked --release --manifest-path "$manifest" \
  -p tuilane-runtime-evaluation-guest --target wasm32-unknown-unknown
wasm-tools component new "$guest_wasm" -o "$component"

component_wit=$(wasm-tools component wit "$component")
printf '%s\n' "$component_wit"
imports=$(printf '%s\n' "$component_wit" \
  | sed -n '/^world root {/,/^}/p' \
  | grep -E '^[[:space:]]+import ' \
  | sed 's/^[[:space:]]*//')
if [[ "$imports" != 'import tuilane:runtime-evaluation/host;' ]]; then
  printf 'Unexpected component imports:\n%s\n' "$imports" >&2
  exit 1
fi

TUILANE_RUNTIME_EVALUATION_COMPONENT="$component" \
  rustup run stable cargo run --locked --release --manifest-path "$manifest" \
  -p tuilane-runtime-evaluation-host
