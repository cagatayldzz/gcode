#!/bin/bash
# Record process CPU and resident memory samples without instrumenting the application.
set -euo pipefail

pid=${1:-}
duration=${2:-60}

if [[ ! "$pid" =~ ^[1-9][0-9]*$ || ! "$duration" =~ ^[1-9][0-9]*$ || $# -gt 2 ]]; then
  printf 'Usage: bash scripts/profile-vekro.sh PID [SECONDS]\n' >&2
  exit 2
fi

export LC_ALL=C
printf 'elapsed_seconds,cpu_percent,rss_kib\n'
SECONDS=0
for ((index = 0; index < duration; index++)); do
  if ! sample=$(ps -p "$pid" -o %cpu= -o rss=) || ! read -r cpu rss <<< "$sample" || [[ -z "$cpu" || -z "$rss" ]]; then
    printf 'Could not sample process %s. Check that it is running and that ps can read it.\n' "$pid" >&2
    exit 1
  fi
  printf '%s,%s,%s\n' "$SECONDS" "$cpu" "$rss"
  if (( index + 1 < duration )); then
    sleep 1
  fi
done
