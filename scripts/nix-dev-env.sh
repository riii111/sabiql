#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/nix-dev-env.sh diagnose
  scripts/nix-dev-env.sh clean --confirm

diagnose       Show the repository-local direnv cache and matching GC roots.
clean          Remove only this repository's .direnv cache.
  --confirm     Required for clean.
EOF
}

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo_root=$(cd -- "$script_dir/.." && pwd -P)
git_root=$(git -C "$repo_root" rev-parse --show-toplevel 2>/dev/null) || {
  echo "Run this command from inside the sabiql repository." >&2
  exit 1
}

if [[ "$git_root" != "$repo_root" ]]; then
  echo "The diagnostic script must remain inside the sabiql repository." >&2
  exit 1
fi

layout_dir="$repo_root/.direnv"
command_name="${1:-diagnose}"
confirm=0

if [[ $# -gt 0 ]]; then
  shift
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --confirm)
      confirm=1
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

print_versions() {
  if command -v nix >/dev/null 2>&1; then
    nix --version
  else
    echo "nix: unavailable"
  fi

  if command -v direnv >/dev/null 2>&1; then
    printf 'direnv: '
    direnv version
  else
    echo "direnv: unavailable"
  fi
}

print_cache() {
  if [[ ! -d "$layout_dir" ]]; then
    echo ".direnv: absent"
    return
  fi

  echo ".direnv entries:"
  find "$layout_dir" -mindepth 1 -maxdepth 1 -print | sort

  local temporary_profiles
  temporary_profiles=$(find "$layout_dir" -mindepth 1 -maxdepth 1 \
    \( -name 'flake-tmp-profile.*' -o -name 'nix-tmp-profile.*' \) -print)
  if [[ -n "$temporary_profiles" ]]; then
    echo "temporary profiles:"
    printf '%s\n' "$temporary_profiles"
  else
    echo "temporary profiles: none"
  fi
}

print_gc_roots() {
  local roots matching_roots
  if ! command -v nix-store >/dev/null 2>&1; then
    echo "GC roots: nix-store unavailable"
  elif ! roots=$(nix-store --gc --print-roots 2>/dev/null); then
    echo "GC roots: unavailable (Nix daemon access is required)"
  else
    matching_roots=$(printf '%s\n' "$roots" | grep -F -- "$repo_root" || true)
    if [[ -n "$matching_roots" ]]; then
      echo "GC roots referencing this repository:"
      printf '%s\n' "$matching_roots"
    else
      echo "GC roots referencing this repository: none"
    fi
  fi
}

diagnose() {
  print_versions
  print_cache
  print_gc_roots
}

clean() {
  if [[ $confirm -ne 1 ]]; then
    echo "Refusing to remove $layout_dir without --confirm." >&2
    exit 2
  fi

  if [[ "${DIRENV_DIR:-}" == "$repo_root" || "${DIRENV_DIR:-}" == "-$repo_root" ]]; then
    echo "Exit the direnv-loaded shell before cleaning $layout_dir." >&2
    exit 2
  fi

  if [[ -e "$layout_dir" || -L "$layout_dir" ]]; then
    rm -rf -- "$layout_dir"
    echo "Removed repository-local direnv cache: $layout_dir"
    echo "Repository-local temporary profiles and generation links were discarded."
  else
    echo "Repository-local direnv cache is already absent: $layout_dir"
    echo "No repository-local temporary profiles or generation links were present."
  fi
  echo "User/system profile generations and rollback history were not changed."
  echo "Run 'direnv allow' to rebuild it."
}

case "$command_name" in
  diagnose)
    if [[ $confirm -ne 0 ]]; then
      usage >&2
      exit 2
    fi
    diagnose
    ;;
  clean)
    clean
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
