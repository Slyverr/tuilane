#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  printf 'Usage: %s <previous-binary> <new-binary>\n' "$0" >&2
  exit 2
fi

previous_binary=$(realpath -- "$1")
new_binary=$(realpath -- "$2")
for binary in "$previous_binary" "$new_binary"; do
  if [[ ! -x "$binary" ]]; then
    printf 'Upgrade-test binary is missing or not executable: %s\n' "$binary" >&2
    exit 1
  fi
done

temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/tuilane-upgrade-check.XXXXXXXX")
trap 'rm -rf -- "$temporary_directory"' EXIT

installation_directory="$temporary_directory/install/bin"
mkdir -p "$installation_directory"
installed_binary="$installation_directory/tuilane"
install -m 755 "$previous_binary" "$installed_binary"
export TUILANE_DATA_DIR="$temporary_directory/data"
export XDG_CONFIG_HOME="$temporary_directory/config"

"$installed_binary" task create --title 'data created before upgrade' >/dev/null

install -m 755 "$new_binary" "$installed_binary.new"
mv -- "$installed_binary.new" "$installed_binary"

if ! "$installed_binary" task list | grep -Fq 'data created before upgrade'; then
  printf 'User task data was not available after replacing the binary\n' >&2
  exit 1
fi
if ! "$installed_binary" doctor | grep -Fq 'Database: healthy'; then
  printf 'Upgraded executable did not report healthy local storage\n' >&2
  exit 1
fi

printf 'Validated data preservation from %s to %s\n' \
  "$($previous_binary --version)" "$($new_binary --version)"
