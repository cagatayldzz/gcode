#!/usr/bin/env bash
# Syntax sample: Shell.
set -euo pipefail

readonly NAME="${1:-world}"
COUNT=3
declare -a ITEMS=(one two "three four")
declare -A MAP=([key]=value)

greet() {
  local who="$1"
  echo "Hello, ${who}! You are number $((COUNT + 1))."
}

usage() {
  cat <<USAGE
Usage: $0 [options] <name>
  -h  show help
USAGE
}

if [[ -z "$NAME" || ! -f "$NAME" ]] && [ "$COUNT" -gt 2 ]; then
  greet "$NAME"
elif command -v git >/dev/null 2>&1; then
  echo 'single quoted $NOT_EXPANDED'
else
  usage >&2
  exit 1
fi

for item in "${ITEMS[@]}"; do
  case "$item" in
    one | two) echo "small: $item" ;;
    *) echo "other: $item" ;;
  esac
done

for ((i = 0; i < COUNT; i++)); do
  printf '%s\n' "$i"
done

while read -r line; do
  [[ "$line" =~ ^[0-9]+$ ]] && continue
  echo "$line" | tr 'a-z' 'A-Z' | sed -e 's/foo/bar/g'
done < <(ls -la "$HOME")

result="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
export RESULT="$result" PATH="$HOME/bin:$PATH"
trap 'echo "cleaning up"' EXIT
