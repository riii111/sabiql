#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
cleanup_script="$script_dir/nix-direnv-profile-cleanup.sh"
nix_dev_env_script="$script_dir/nix-dev-env.sh"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

profile_dir="$tmp_dir/.direnv"
mkdir -p "$profile_dir"
live_pid=$$
dead_pid=999999999
touch \
  "$profile_dir/flake-tmp-profile.$live_pid" \
  "$profile_dir/flake-tmp-profile.$live_pid-1-link" \
  "$profile_dir/flake-tmp-profile.$dead_pid" \
  "$profile_dir/flake-tmp-profile.$dead_pid-1-link" \
  "$profile_dir/flake-tmp-profile.invalid"

"$cleanup_script" "$profile_dir"

[[ -e "$profile_dir/flake-tmp-profile.$live_pid" ]]
[[ -e "$profile_dir/flake-tmp-profile.$live_pid-1-link" ]]
[[ ! -e "$profile_dir/flake-tmp-profile.$dead_pid" ]]
[[ ! -e "$profile_dir/flake-tmp-profile.$dead_pid-1-link" ]]
[[ ! -e "$profile_dir/flake-tmp-profile.invalid" ]]

repo_fixture="$tmp_dir/repo"
caller_dir="$tmp_dir/caller"
mkdir -p "$repo_fixture/scripts" "$repo_fixture/.direnv" "$caller_dir/.direnv"
cp "$nix_dev_env_script" "$repo_fixture/scripts/nix-dev-env.sh"
git -C "$repo_fixture" init -q
touch "$repo_fixture/.direnv/repository-cache" "$caller_dir/.direnv/caller-cache"

(
  cd "$caller_dir"
  "$repo_fixture/scripts/nix-dev-env.sh" clean --confirm >/dev/null
)

[[ ! -e "$repo_fixture/.direnv/repository-cache" ]]
[[ -e "$caller_dir/.direnv/caller-cache" ]]
