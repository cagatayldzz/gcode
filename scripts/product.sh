#!/bin/bash
# Prints one value of product.toml, the single place the product's name is written.
#   scripts/product.sh name         the name people see (the app bundle, its executable)
#   scripts/product.sh slug         the name in lower case, for file names
#   scripts/product.sh bundle_id    the macOS bundle identifier
# Other scripts call it instead of writing the name themselves.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
key="${1:?Usage: scripts/product.sh name|slug|env_prefix|bundle_id|legacy_name}"

value() {
  sed -n "s/^$1 = \"\([^\"]*\)\".*/\1/p" "$root/product.toml" | head -n 1
}

if [[ "$key" == slug ]]; then
  value name | tr '[:upper:]' '[:lower:]'
else
  result="$(value "$key")"
  [[ -n "$result" ]] || { echo "product.toml has no $key" >&2; exit 1; }
  echo "$result"
fi
