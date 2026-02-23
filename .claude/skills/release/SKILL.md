---
name: release
description: Release workflow for sabiql - version bump, tag, and GitHub release
disable-model-invocation: true
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

4. **Create GitHub Release with release notes**
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --generate-notes --notes "$(cat <<'EOF'
   ## What's Changed in vX.Y.Z

   ### 🎯 User-Facing Changes
   > Features and fixes that affect how you use sabiql.

   - **Feature A**: description
   - **Fix B**: description

   ### 🔧 Internal Improvements
   > Refactoring and tests — no behavior change for end users.

   - **Refactor C**: description
   - **Test D**: description
   EOF
   )"
   ```
   - Categorize commits into **User-Facing Changes** vs **Internal Improvements**
   - User-Facing: new features, behavior changes, bug fixes, dependency upgrades that affect UX
   - Internal: refactoring, test additions, CI/docs changes, rule/skill updates

5. **Verify release**
   - GitHub Actions automatically builds and publishes binaries to Releases
   - Check https://github.com/riii111/sabiql/releases for the new release
