#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

cargo_target_dir=${CARGO_TARGET_DIR:-"$repo_root/target"}
plugin_target_dir="$cargo_target_dir/plugin-sdk"
bundle_dir="$plugin_target_dir/hello-world-bundle"
wasm_guest="$plugin_target_dir/wasm32-unknown-unknown/release/tuilane_hello_world_plugin.wasm"
component="$plugin_target_dir/hello-world.component.wasm"
binary="$cargo_target_dir/debug/tuilane"

if ! rustup target list --installed --toolchain stable | grep -Fxq wasm32-unknown-unknown; then
  printf '%s\n' 'Missing Rust target wasm32-unknown-unknown.' \
    'Install it with: rustup target add wasm32-unknown-unknown --toolchain stable' >&2
  exit 1
fi

cargo build -p tuilane --locked
cargo fmt --manifest-path examples/plugins/hello-world/Cargo.toml --all -- --check
cargo build --locked --release \
  --manifest-path examples/plugins/hello-world/Cargo.toml \
  --target wasm32-unknown-unknown \
  --target-dir "$plugin_target_dir"

mkdir -p "$bundle_dir"
wasm-tools component new "$wasm_guest" -o "$component"
cp examples/plugins/hello-world/manifest.toml "$bundle_dir/manifest.toml"
cp "$component" "$bundle_dir/plugin.wasm"

TUILANE_PLUGIN_TEST_BUNDLE="$bundle_dir" \
  cargo test -p tuilane-plugin-runtime --test plugin_lifecycle --locked -- --ignored
TUILANE_PLUGIN_TEST_BUNDLE="$bundle_dir" \
  cargo test -p tuilane --bin tuilane --locked external_plugin_view_renders -- --ignored
data_dir=$(mktemp -d "$plugin_target_dir/cli-data.XXXXXXXX")

cleanup() {
  rm -rf "$data_dir"
}
trap cleanup EXIT

export TUILANE_DATA_DIR="$data_dir"
"$binary" plugin list
"$binary" plugin install "$bundle_dir"
"$binary" plugin enable org.example.hello
commands=$("$binary" plugin commands org.example.hello)
grep -Fq 'org.example.hello.greet@1' <<<"$commands"
permissions=$("$binary" plugin permissions org.example.hello)
grep -Fq 'workspace.info@1' <<<"$permissions"
grep -Fq 'not granted' <<<"$permissions"

if "$binary" plugin call org.example.hello org.example.hello.greet@1 \
  --request '{"name":"Ada"}' >/dev/null 2>&1; then
  printf '%s\n' 'Plugin service call succeeded without a permission grant.' >&2
  exit 1
fi

"$binary" plugin grant org.example.hello workspace.read
greeting=$("$binary" plugin call org.example.hello org.example.hello.greet@1 \
  --request '{"name":"Ada"}')
grep -Fq 'Hello, Ada!' <<<"$greeting"
grep -Fq 'tuilane' <<<"$greeting"

"$binary" task create --title 'plugin sdk workspace search marker'
if "$binary" plugin call org.example.hello org.example.hello.search@1 \
  --request '{"query":"plugin sdk workspace search marker","kind":"task"}' >/dev/null 2>&1; then
  printf '%s\n' 'Plugin search succeeded without a permission grant.' >&2
  exit 1
fi

"$binary" plugin grant org.example.hello workspace.search
search_results=$("$binary" plugin call org.example.hello org.example.hello.search@1 \
  --request '{"query":"plugin sdk workspace search marker","kind":"task"}')
grep -Fq 'plugin sdk workspace search marker' <<<"$search_results"
"$binary" plugin revoke org.example.hello workspace.read
if "$binary" plugin call org.example.hello org.example.hello.greet@1 \
  --request '{"name":"Ada"}' >/dev/null 2>&1; then
  printf '%s\n' 'Plugin service call succeeded after its permission was revoked.' >&2
  exit 1
fi

plugin_data="$data_dir/plugins/data/org.example.hello"
mkdir -p "$plugin_data"
printf '%s\n' 'preserve this file' >"$plugin_data/retained.txt"
"$binary" plugin disable org.example.hello
"$binary" plugin uninstall org.example.hello
test -f "$plugin_data/retained.txt"
