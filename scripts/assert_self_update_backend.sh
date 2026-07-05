#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

backend_refs="$(grep -R -n --include='*.rs' 'self_update::backends::' src || true)"

if [[ -z "$backend_refs" ]]; then
  echo "Expected at least one self_update backend reference."
  exit 1
fi

unexpected_refs="$(printf '%s\n' "$backend_refs" | grep -v 'self_update::backends::github' || true)"

if [[ -n "$unexpected_refs" ]]; then
  echo "Only self_update::backends::github is allowed while quick-xml advisories are ignored."
  printf '%s\n' "$unexpected_refs"
  exit 1
fi
