---
name: release
description: Release workflow for sabiql - version bump, tag, and GitHub release
---

# Release Workflow

Use this skill when releasing a new version of sabiql.

## Steps

1. **Update version in `Cargo.toml`**
   ```toml
   [package]
   version = "X.Y.Z"
   ```

2. **Commit and push to main**
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "chore: bump version to vX.Y.Z"
   git push origin main
   ```

3. **Create and push tag**
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

4. **Verify release**
   - GitHub Actions automatically builds and publishes binaries to Releases
   - Check https://github.com/riii111/sabiql/releases for the new release
