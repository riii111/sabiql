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

2. **Sync `Cargo.lock`**
   ```bash
   cargo generate-lockfile
   ```

3. **Commit and push to main**
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "chore: bump version to vX.Y.Z"
   git push origin main
   ```

4. **Create and push tag**
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

5. **Create GitHub Release with release notes**
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --notes "$(cat <<'EOF'
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

6. **Verify release (すべて成功するまで監視する)**

   以下のワークフローを順に監視し、すべて成功することを確認する。
   失敗した場合は原因を調査し、タグ再作成を含めて対応する。

   | # | ワークフロー | リポジトリ | トリガー | 確認内容 |
   |---|-------------|-----------|---------|---------|
   | 1 | **Release** (ビルド + バイナリ公開 + crates.io) | `riii111/sabiql` | タグpush | 全ターゲットのビルド成功、crates.io publish 成功 |
   | 2 | **update-homebrew-tap** (formula PR作成) | `riii111/sabiql` | Release成功 | `riii111/homebrew-sabiql` にPRが作られること |
   | 3 | **brew test-bot** (bottle ビルド) | `riii111/homebrew-sabiql` | formula PRに対して自動実行 | `--only-formulae` が実行され bottle artifact がアップロードされること |
   | 4 | **Publish bottles** | `riii111/homebrew-sabiql` | brew test-bot (PR) 成功後、PRマージで発火 | bottle コミットが main に push されること |

   ```bash
   # 1. Release ワークフローを監視
   gh run watch <run-id> --exit-status

   # 2. homebrew-sabiql に PR が作られたことを確認
   gh pr view <number> --repo riii111/homebrew-sabiql

   # 3. brew test-bot (PR イベント) を監視 — bottle ビルドされることを確認
   gh run watch <run-id> --repo riii111/homebrew-sabiql --exit-status

   # 4. PR をマージし、Publish bottles を監視
   gh pr merge <number> --repo riii111/homebrew-sabiql --merge
   gh run watch <run-id> --repo riii111/homebrew-sabiql --exit-status
   ```

   **注意:**
   - homebrew-sabiql の formula 更新は必ず **PR 経由** でマージすること (main への直push では bottle がビルドされない)
   - brew test-bot で `--only-formulae` がスキップされていたら PR イベントで走っていない可能性がある
