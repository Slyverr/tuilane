#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <release-archive.tar.gz>\n' "$0" >&2
  exit 2
fi

archive=$(realpath -- "$1")
checksum="${archive}.sha256"
if [[ ! -f "$archive" || ! -f "$checksum" ]]; then
  printf 'Release archive or checksum is missing: %s\n' "$archive" >&2
  exit 1
fi

(cd "$(dirname -- "$archive")" && sha256sum --check "$(basename -- "$checksum")")

temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/tuilane-package-check.XXXXXXXX")
trap 'rm -rf -- "$temporary_directory"' EXIT
tar -xzf "$archive" -C "$temporary_directory"

bundle_name=$(basename -- "$archive" .tar.gz)
bundle_directory="$temporary_directory/$bundle_name"
archive_binary="$bundle_directory/bin/tuilane"
for file in \
  "$archive_binary" \
  "$bundle_directory/README.md" \
  "$bundle_directory/CHANGELOG.md" \
  "$bundle_directory/LICENSE-MIT" \
  "$bundle_directory/LICENSE-APACHE" \
  "$bundle_directory/BUILD-INFO.txt"; do
  if [[ ! -f "$file" ]]; then
    printf 'Release archive is missing required file: %s\n' "${file#"$bundle_directory"/}" >&2
    exit 1
  fi
done

if [[ ! -x "$archive_binary" ]]; then
  printf 'Packaged executable is not executable: %s\n' "$archive_binary" >&2
  exit 1
fi

expected_version=${bundle_name#tuilane-}
expected_version=${expected_version%-x86_64-unknown-linux-gnu}
actual_version=$("$archive_binary" --version)
if [[ "$actual_version" != "tuilane $expected_version" ]]; then
  printf 'Packaged executable reports %s (expected tuilane %s)\n' \
    "$actual_version" "$expected_version" >&2
  exit 1
fi

grep -Fxq "version=$expected_version" "$bundle_directory/BUILD-INFO.txt"
grep -Fxq 'target=x86_64-unknown-linux-gnu' "$bundle_directory/BUILD-INFO.txt"
source_commit=$(sed -n 's/^source_commit=//p' "$bundle_directory/BUILD-INFO.txt")
if [[ ! "$source_commit" =~ ^[0-9a-f]{40}$ ]]; then
  printf 'Release archive contains an invalid source commit identifier\n' >&2
  exit 1
fi

if ! "$archive_binary" --help | grep -Fq 'Usage: tuilane'; then
  printf 'Packaged executable did not return valid CLI help\n' >&2
  exit 1
fi

user_home="$temporary_directory/home"
install_directory="$user_home/.local/bin"
installed_binary="$install_directory/tuilane"
mkdir -p "$install_directory"
install -m 755 "$archive_binary" "$installed_binary"

export HOME="$user_home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export TUILANE_DATA_DIR="$XDG_DATA_HOME/tuilane"
export PATH="$install_directory:$PATH"

if [[ "$(command -v tuilane)" != "$installed_binary" ]]; then
  printf 'Installed tuilane command does not resolve from the temporary PATH\n' >&2
  exit 1
fi

if [[ "$(tuilane --version)" != "tuilane $expected_version" ]]; then
  printf 'Installed executable reports an unexpected version\n' >&2
  exit 1
fi

tuilane task create --title 'release package smoke test' >/dev/null
if ! tuilane task list | grep -Fq 'release package smoke test'; then
  printf 'Packaged executable could not read back its local task data\n' >&2
  exit 1
fi
if ! tuilane doctor | grep -Fq 'Database: healthy'; then
  printf 'Packaged executable did not report healthy local storage\n' >&2
  exit 1
fi

printf 'Validated clean installation from %s\n' "$(basename -- "$archive")"
