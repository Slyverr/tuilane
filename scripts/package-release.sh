#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  printf 'Usage: %s <target-triple> <binary> <output-directory>\n' "$0" >&2
  exit 2
fi

target=$1
binary=$2
output_directory=$3

if [[ "$target" != x86_64-unknown-linux-gnu ]]; then
  printf 'Unsupported release target: %s\n' "$target" >&2
  exit 2
fi

repo_root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ ! -x "$binary" ]]; then
  printf 'Release binary is missing or not executable: %s\n' "$binary" >&2
  exit 1
fi

workspace_version=$(cargo metadata --no-deps --locked --format-version 1 \
  | python3 -c '
import json
import sys

packages = [
    package
    for package in json.load(sys.stdin)["packages"]
    if package["name"] == "tuilane"
]
if len(packages) != 1:
    sys.exit("Expected exactly one tuilane package in Cargo metadata")
print(packages[0]["version"])
')
source_commit=$(git rev-parse HEAD)
binary_version=$("$binary" --version)
if [[ "$binary_version" != "tuilane $workspace_version" ]]; then
  printf 'Binary version does not match the workspace: %s (expected tuilane %s)\n' \
    "$binary_version" "$workspace_version" >&2
  exit 1
fi

mkdir -p "$output_directory"
output_directory=$(CDPATH='' cd -- "$output_directory" && pwd)
archive_name="tuilane-${workspace_version}-${target}.tar.gz"
checksum_name="${archive_name}.sha256"
archive_path="$output_directory/$archive_name"
checksum_path="$output_directory/$checksum_name"

if [[ -e "$archive_path" || -e "$checksum_path" ]]; then
  printf 'Refusing to overwrite an existing release artifact in %s\n' \
    "$output_directory" >&2
  exit 1
fi

source_date_epoch=${SOURCE_DATE_EPOCH:-0}
if [[ ! "$source_date_epoch" =~ ^[0-9]+$ ]]; then
  printf 'SOURCE_DATE_EPOCH must be a non-negative integer\n' >&2
  exit 2
fi

temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/tuilane-release.XXXXXXXX")
trap 'rm -rf -- "$temporary_directory"' EXIT

bundle_name="tuilane-${workspace_version}-${target}"
bundle_directory="$temporary_directory/$bundle_name"
install -D -m 755 "$binary" "$bundle_directory/bin/tuilane"
install -D -m 644 docs/release-archive.md "$bundle_directory/README.md"
install -D -m 644 CHANGELOG.md "$bundle_directory/CHANGELOG.md"
install -D -m 644 LICENSE-MIT "$bundle_directory/LICENSE-MIT"
install -D -m 644 LICENSE-APACHE "$bundle_directory/LICENSE-APACHE"
printf 'version=%s\nsource_commit=%s\ntarget=%s\n' \
  "$workspace_version" "$source_commit" "$target" \
  >"$bundle_directory/BUILD-INFO.txt"

tar \
  --sort=name \
  --mtime="@$source_date_epoch" \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -C "$temporary_directory" \
  -cf - "$bundle_name" \
  | gzip -n >"$temporary_directory/$archive_name"

(cd "$temporary_directory" && sha256sum -- "$archive_name" >"$checksum_name")
mv -- "$temporary_directory/$archive_name" "$archive_path"
mv -- "$temporary_directory/$checksum_name" "$checksum_path"

printf 'Created %s\n' "$archive_path"
printf 'Created %s\n' "$checksum_path"
