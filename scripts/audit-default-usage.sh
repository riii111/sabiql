#!/usr/bin/env bash
# Audit Default / struct-init patterns for SAB-310.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

count() {
  rg "$@" --glob '*.rs' src/ 2>/dev/null | wc -l | tr -d ' '
}

echo "=== SAB-310 Default usage audit ==="
echo "root: $ROOT"
echo

echo "--- struct update: ..Default::default() ---"
echo "  production (excl #[cfg(test)] blocks — manual review needed):"
rg -n '\.\.Default::default\(\)' src/app src/domain src/infra src/ui src/main.rs \
  --glob '*.rs' --glob '!**/tests/**' 2>/dev/null \
  | rg -v '#\[cfg\(test\)\]|mod tests' || true
echo
echo "  counts by layer (includes in-file test modules):"
for layer in app domain infra ui; do
  c=$(count '\.\.Default::default\(\)' "src/$layer")
  echo "    src/$layer: $c"
done
c=$(count '\.\.Default::default\(\)' src/tests)
echo "    src/tests: $c"
echo

echo "--- impl Default for (manual) ---"
rg -n 'impl Default for' src/ --glob '*.rs'
echo

echo "--- production Type::default() outside tests (sample) ---"
rg -n '[A-Za-z0-9_]+::default\(\)' src/app src/domain src/infra src/ui src/main.rs \
  --glob '*.rs' --glob '!**/tests/**' 2>/dev/null \
  | rg -v 'Style::default|Block::default|ListState::default|ScrollbarState::default|Span::styled' \
  | head -40
echo "  ... (truncated)"
echo

echo "--- unwrap_or_default in infra/app/domain ---"
rg -n 'unwrap_or_default\(\)' src/app src/domain src/infra --glob '*.rs' --glob '!**/tests/**'
echo

echo "--- *self = Self::default() reset ---"
rg -n '\*self = Self::default\(\)' src/ --glob '*.rs' --glob '!**/tests/**'
echo

echo "--- test: empty metadata literals (schemas/table_summaries vec![]) ---"
rg -n 'schemas: vec!\[\]|table_summaries: vec!\[\]' src/app src/tests --glob '*.rs' \
  | rg 'test|tests|fixtures' || rg -n 'schemas: vec!\[\]|table_summaries: vec!\[\]' src/app --glob '*.rs' | head -30
echo

echo "--- test: Column default/comment None noise ---"
rg -c 'default: None' src/app src/tests --glob '*.rs' 2>/dev/null | sort -t: -k2 -nr | head -10
