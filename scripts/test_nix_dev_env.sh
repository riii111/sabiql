#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
cleanup_script="$script_dir/nix-direnv-profile-cleanup.sh"
nix_dev_env_script="$script_dir/nix-dev-env.sh"
nix_store_fixture="$script_dir/test_support/nix-store"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

profile_dir="$tmp_dir/.direnv"
mkdir -p "$profile_dir"
live_pid=$$
dead_pid=999999999
touch "$profile_dir/profile-target"
ln -s profile-target "$profile_dir/flake-tmp-profile.$live_pid"
ln -s profile-target "$profile_dir/flake-tmp-profile.$live_pid-1-link"
ln -s missing-target "$profile_dir/flake-tmp-profile.$dead_pid"
ln -s missing-target "$profile_dir/flake-tmp-profile.$dead_pid-1-link"
ln -s profile-target "$profile_dir/flake-tmp-profile.unknown-format"
touch \
  "$profile_dir/flake-tmp-profile.invalid"

"$cleanup_script" "$profile_dir"

[[ -L "$profile_dir/flake-tmp-profile.$live_pid" ]]
[[ -L "$profile_dir/flake-tmp-profile.$live_pid-1-link" ]]
[[ ! -e "$profile_dir/flake-tmp-profile.$dead_pid" ]]
[[ ! -L "$profile_dir/flake-tmp-profile.$dead_pid" ]]
[[ ! -e "$profile_dir/flake-tmp-profile.$dead_pid-1-link" ]]
[[ ! -L "$profile_dir/flake-tmp-profile.$dead_pid-1-link" ]]
[[ -L "$profile_dir/flake-tmp-profile.unknown-format" ]]
[[ -e "$profile_dir/flake-tmp-profile.invalid" ]]

repo_fixture="$tmp_dir/repo"
caller_dir="$tmp_dir/caller"
mkdir -p "$repo_fixture/scripts" "$repo_fixture/.direnv" "$caller_dir/.direnv"
repo_fixture=$(cd "$repo_fixture" && pwd -P)
caller_dir=$(cd "$caller_dir" && pwd -P)
cp "$nix_dev_env_script" "$repo_fixture/scripts/nix-dev-env.sh"
git -C "$repo_fixture" init -q
touch "$repo_fixture/.direnv/repository-cache" "$caller_dir/.direnv/caller-cache"

(
  cd "$caller_dir"
  "$repo_fixture/scripts/nix-dev-env.sh" clean --confirm
)

[[ ! -e "$repo_fixture/.direnv/repository-cache" ]]
[[ -e "$caller_dir/.direnv/caller-cache" ]]

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"
cp "$nix_store_fixture" "$fake_bin/nix-store"
chmod +x "$fake_bin/nix-store"

matching_roots_output=$(
  PATH="$fake_bin:$PATH" \
    NIX_TEST_GC_ROOTS="$repo_fixture/.direnv"$'\n'"$caller_dir/.direnv" \
    "$repo_fixture/scripts/nix-dev-env.sh" diagnose
)

grep -F "GC roots referencing this repository:" <<<"$matching_roots_output"
grep -F "$repo_fixture/.direnv" <<<"$matching_roots_output"
if grep -F "$caller_dir/.direnv" <<<"$matching_roots_output"; then
  echo "diagnose reported an unrelated GC root" >&2
  exit 1
fi

failed_roots_output=$(
  PATH="$fake_bin:$PATH" \
    NIX_TEST_GC_ROOTS_FAILURE=1 \
    "$repo_fixture/scripts/nix-dev-env.sh" diagnose
)

grep -F "GC roots: unavailable (Nix daemon access is required)" <<<"$failed_roots_output"

missing_cache_output=$(
  "$repo_fixture/scripts/nix-dev-env.sh" clean --confirm
)
grep -F "No repository-local temporary profiles or generation links were present." <<<"$missing_cache_output"
