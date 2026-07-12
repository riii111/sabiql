# Release procedure

## Bump the lockstep release contract

Update the workspace release version and every internal crate dependency lower
bound in `Cargo.toml` together.

```toml
[workspace.package]
version = "X.Y.Z"

[workspace.dependencies]
sabiql-domain = { path = "src/domain", version = "X.Y.Z" }
sabiql-app = { path = "src/app", version = "X.Y.Z" }
sabiql-infra = { path = "src/infra", version = "X.Y.Z" }
sabiql-ui = { path = "src/ui", version = "X.Y.Z" }
```

All publishable internal crates release in lockstep. Their caret lower bounds
must match the workspace version so a published package cannot resolve an
older internal release that lacks its current API.

Regenerate the lockfile and validate the publish contract before committing.

```bash
cargo generate-lockfile
bash scripts/check-publish-order.sh
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to vX.Y.Z"
git push origin main
```

Tag and publish only after the release commit is on `main`.
