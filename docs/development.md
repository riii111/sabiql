# Development environment

## Nix and direnv

Automatic loading through `.envrc` requires `nix-direnv` 3.1 or newer. Install `direnv` and `nix-direnv` into the user profile when needed:

```bash
nix profile install nixpkgs#direnv nixpkgs#nix-direnv
```

Source the installed `nix-direnv` configuration from `~/.config/direnv/direnvrc`:

```bash
source "$HOME/.nix-profile/share/nix-direnv/direnvrc"
```

The normal flow is:

```bash
direnv allow
cargo nextest run --workspace
nix build
```

Use `nix develop` to enter the development shell explicitly when `direnv` is unavailable.

## Diagnosis and cleanup

The repository-local cache and GC roots that reference the current worktree can be inspected without changing the Nix store:

```bash
./scripts/nix-dev-env.sh diagnose
```

For manual inspection of indirect roots, list the symlinks under the Nix state directory. Do not remove them from this repository script:

```bash
auto_gc_roots_dir="${NIX_STATE_DIR:-/nix/var/nix}/gcroots/auto"
for root in "$auto_gc_roots_dir"/*; do
  [[ -L "$root" ]] || continue
  printf '%s -> %s\n' "$root" "$(readlink "$root")"
done
```

Nix ignores dangling indirect roots during collection, so they are not included in `nix-store --gc --print-roots`; verify each target before any manual cleanup. See the [Nix reference manual](https://nix.dev/manual/nix/latest/command-ref/nix-store/gc) for the root lifecycle.

Exit shells using this repository before cleaning a stale cache. `--confirm` is required:

```bash
./scripts/nix-dev-env.sh clean --confirm
direnv allow
```

Cleanup removes only this repository's `.direnv` cache. This discards repository-local temporary profiles and their generation links, including those left by interrupted evaluations. It does not modify user or system profile generations, affect their `nix profile rollback` history, or run system-wide garbage collection. Automatic cleanup removes only recognised profile names whose PID is no longer running; unknown names remain for diagnosis or manual cleanup. The `.envrc` cleanup is a compatibility shim for interrupted evaluations until nix-direnv provides equivalent upstream trap cleanup.

Review global storage separately with `nix-store --gc --print-roots` and `nix store gc --dry-run` before taking any system-wide cleanup action.
