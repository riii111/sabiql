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

5. **Create draft GitHub Release with release notes**
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --draft --notes "$(cat <<'EOF'
   ## Highlights

   A short narrative summary of the key changes in this release.
   Write 1-3 paragraphs in natural language, focusing on what users
   can now do differently. Mention contributors with @mentions.

   ## What's Changed in vX.Y.Z

   ### 🎯 User-Facing Changes
   > Features and fixes that affect how you use sabiql.

   - feat(sab-XX): description by @author in #123
   - fix: description by @author in #456

   ### 🔧 Internal Improvements
   > Refactoring and tests — no behavior change for end users.

   - refactor(sab-XX): description by @author in #789
   - test: description by @author in #012
   EOF
   )"
   ```
   - `--draft` で作成する。CI がビルド成功後にドラフトを解除してバイナリを添付する
   - **Highlights**: User-Facing Changes を自然言語で要約する。読者が PR 一覧を読まなくても対応内容を把握できるようにする
   - Categorize commits into **User-Facing Changes** vs **Internal Improvements**
   - User-Facing: new features, behavior changes, bug fixes, dependency upgrades that affect UX
   - Internal: refactoring, test additions, CI/docs changes, rule/skill updates
   - Each entry follows: `- <commit title without PR number> by @<author> in #<PR number>`
   - Use `git log vPREV..vX.Y.Z --oneline` to get commit list, then look up PR numbers and authors

6. **Verify release (すべて成功するまで監視する)**

   以下のワークフローを順に監視し、すべて成功することを確認する。

   | # | ワークフロー | リポジトリ | トリガー | 確認内容 |
   |---|-------------|-----------|---------|---------|
   | 1 | **Release** (ビルド + バイナリ公開 + crates.io) | `riii111/sabiql` | タグpush | 全ターゲットのビルド成功、crates.io publish 成功 |
   | 2 | **update-homebrew-tap** (formula PR作成) | `riii111/sabiql` | Release成功 | `riii111/homebrew-sabiql` に `bump-sabiql-vX.Y.Z` ブランチでPRが作られること |
   | 3 | **brew test-bot** (bottle ビルド) | `riii111/homebrew-sabiql` | formula PRに対して自動実行 | `--only-formulae` が実行され bottle artifact がアップロードされること |
   | 4 | **Publish bottles** | `riii111/homebrew-sabiql` | brew test-bot (PR) 成功後、PRマージで発火 | `Pull bottles` と `Push commits` が **実行** (skipped でない) され、bottle コミットが main に存在すること |

   ```bash
   # 1. Release ワークフローを監視
   gh run watch <run-id> --exit-status

   # 2. homebrew-sabiql に PR が作られたことを確認
   gh pr view <number> --repo riii111/homebrew-sabiql

   # 3. brew test-bot (PR イベント) を監視
   gh run watch <run-id> --repo riii111/homebrew-sabiql --exit-status

   # 4. PR をマージし、Publish bottles を監視
   gh pr merge <number> --repo riii111/homebrew-sabiql --merge
   gh run watch <run-id> --repo riii111/homebrew-sabiql --exit-status

   # 5. bottle コミットが main に入ったことを最終確認
   gh api repos/riii111/homebrew-sabiql/commits --jq '.[0].commit.message'
   # => "sabiql: add X.Y.Z bottle." であること
   ```

   **注意:**
   - homebrew-sabiql の formula 更新は必ず **PR 経由** でマージすること (main への直push では `--only-formulae` が実行されず bottle がビルドされない)
   - PRのブランチ名は `bump-sabiql-vX.Y.Z` であること (Publish bottles が `startsWith(branch, 'bump-sabiql-')` で判定している)
   - Publish bottles は全ステップ skipped でも conclusion=success になる。`Pull bottles` / `Push commits` が実際に実行されたかジョブ詳細で確認すること

7. **Release CI 失敗時のリカバリ**

   Release ワークフローが失敗した場合、原因を修正してタグを打ち直す。

   ```bash
   # 1. 既存のリリースとタグを削除
   gh release delete vX.Y.Z --yes
   git tag -d vX.Y.Z
   git push origin :refs/tags/vX.Y.Z

   # 2. 修正をコミット・push
   git push origin main

   # 3. タグを再作成・push (Step 4-6 をやり直す)
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

