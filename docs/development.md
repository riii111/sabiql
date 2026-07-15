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

To inspect every GC root, including roots left behind by deleted worktrees, use:

```bash
./scripts/nix-dev-env.sh diagnose --all-gc-roots
```

Exit shells using this repository before cleaning a stale cache. `--confirm` is required:

```bash
./scripts/nix-dev-env.sh clean --confirm
direnv allow
```

Cleanup removes only this repository's `.direnv` cache. It does not delete Nix profile generations, affect `nix profile rollback`, or run system-wide garbage collection. Interrupted temporary profiles are removed only when their PID is no longer running; the profile and generation links for a running evaluation are retained.

Review global storage separately with `nix-store --gc --print-roots` and `nix store gc --dry-run` before taking any system-wide cleanup action.
