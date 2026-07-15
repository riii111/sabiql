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
cp "$nix_dev_env_script" "$repo_fixture/scripts/nix-dev-env.sh"
git -C "$repo_fixture" init -q
touch "$repo_fixture/.direnv/repository-cache" "$caller_dir/.direnv/caller-cache"

(
  cd "$caller_dir"
  "$repo_fixture/scripts/nix-dev-env.sh" clean --confirm >/dev/null
)

[[ ! -e "$repo_fixture/.direnv/repository-cache" ]]
[[ -e "$caller_dir/.direnv/caller-cache" ]]

fake_bin="$tmp_dir/bin"
auto_root_dir="$tmp_dir/nix-state/gcroots/auto"
mkdir -p "$fake_bin" "$auto_root_dir"
cp "$nix_store_fixture" "$fake_bin/nix-store"
chmod +x "$fake_bin/nix-store"
ln -s "$caller_dir/deleted-worktree" "$auto_root_dir/stale-worktree"

all_gc_roots_output=$(
  PATH="$fake_bin:$PATH" \
    NIX_STATE_DIR="$tmp_dir/nix-state" \
    NIX_TEST_GC_ROOTS="$repo_fixture/.direnv" \
    "$repo_fixture/scripts/nix-dev-env.sh" diagnose --all-gc-roots
)

grep -F "$auto_root_dir/stale-worktree -> $caller_dir/deleted-worktree" <<<"$all_gc_roots_output"
