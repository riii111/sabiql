#!/usr/bin/env bash
set -euo pipefail

# Run all custom lints in parallel.
# Each lint script must exit 0 on success, non-zero on failure.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

lints=(
  "$SCRIPT_DIR/lint_test_names.sh"
  "$SCRIPT_DIR/lint_visible_result.sh"
)

pids=()
names=()

for lint in "${lints[@]}"; do
  name="$(basename "$lint" .sh)"
  "$lint" &
  pids+=($!)
  names+=("$name")
done

failed=0
for i in "${!pids[@]}"; do
  if ! wait "${pids[$i]}"; then
    echo "❌ ${names[$i]} failed" >&2
    failed=1
  fi
done

exit $failed
