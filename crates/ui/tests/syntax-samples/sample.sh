#!/usr/bin/env bash
# Syntax sample: Shell.
set -euo pipefail

NAME="${1:-world}"

greet() {
  echo "Hello, $NAME!"
}

if [ -f "./file.txt" ]; then
  greet | tr 'a-z' 'A-Z'
fi

for i in {1..3}; do
  echo "$i"
done
