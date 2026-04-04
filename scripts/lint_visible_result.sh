#!/usr/bin/env bash
set -euo pipefail

# Lint: reducers must use visible_result() instead of current_result().
#
# current_result() returns the live result regardless of history mode.
# Reducers that read result data should use visible_result() so that
# history-mode interactions operate on the displayed result.
#
# Allowed exceptions (owner-write / cache-save patterns):
#   - connection/helpers.rs  (cache round-trip, needs the live value)
#   - #[cfg(test)] blocks    (test setup/assertions)

SEARCH_DIR="src/app/update"
PATTERN='\.current_result\(\)'

# Files where current_result() is legitimate (owner write / cache)
ALLOW=(
  "src/app/update/connection/helpers.rs"
)

errors=()

while IFS= read -r line; do
  file="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"

  # Check allow-list
  skip=false
  for allowed in "${ALLOW[@]}"; do
    if [[ "$file" == *"$allowed" ]]; then
      skip=true
      break
    fi
  done
  $skip && continue

  # Skip test code: if the file has `#[cfg(test)]` before this line,
  # assume everything after it is test code.
  if [[ -f "$file" ]]; then
    test_start=$(rg -n '#\[cfg\(test\)\]' "$file" 2>/dev/null | head -1 | cut -d: -f1 || echo "")
    if [[ -n "$test_start" && "$lineno" -ge "$test_start" ]]; then
      continue
    fi
  fi

  errors+=("$file:$lineno: use visible_result() instead of current_result() in reducer code")
done < <(rg -n "$PATTERN" "$SEARCH_DIR" 2>/dev/null || true)

if [[ ${#errors[@]} -eq 0 ]]; then
  echo "visible-result lint passed"
else
  echo "visible-result lint failed:" >&2
  for err in "${errors[@]}"; do
    echo "- $err" >&2
  done
  exit 1
fi
