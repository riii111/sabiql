#!/usr/bin/env bash
set -euo pipefail

layout_dir="${1:-.direnv}"

if [[ $# -gt 1 ]]; then
  echo "Usage: scripts/nix-direnv-profile-cleanup.sh [layout-dir]" >&2
  exit 2
fi

[[ -d "$layout_dir" ]] || exit 0

active_prefixes=""
for profile in "$layout_dir"/flake-tmp-profile.*; do
  [[ -e "$profile" || -L "$profile" ]] || continue
  profile_name=${profile##*/}
  if [[ "$profile_name" =~ ^(flake-tmp-profile\.([0-9]+))(-[0-9]+-link)?$ ]]; then
    profile_prefix=${BASH_REMATCH[1]}
    pid=${BASH_REMATCH[2]}
    if kill -0 "$pid" 2>/dev/null; then
      active_prefixes+=$'\n'"$profile_prefix"$'\n'
    fi
  fi
done

for profile in "$layout_dir"/flake-tmp-profile.*; do
  [[ -e "$profile" || -L "$profile" ]] || continue
  profile_name=${profile##*/}
  if [[ "$profile_name" =~ ^(flake-tmp-profile\.([0-9]+))(-[0-9]+-link)?$ ]]; then
    profile_prefix=${BASH_REMATCH[1]}
    case $'\n'"$active_prefixes"$'\n' in
      *$'\n'"$profile_prefix"$'\n'*) continue ;;
    esac
  fi
  rm -f -- "$profile"
done
