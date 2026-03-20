---
paths:
  - "**/tests/render_snapshots/**"
---

# ビジュアルリグレッションテスト

## 概要

- **ライブラリ**: [insta](https://insta.rs) - Rust スナップショットテスト
- **スコープ**: `AppState` → `MainLayout::render()` の統合テスト
- **バックエンド**: Ratatui `TestBackend`（インメモリターミナル 80x24）

## ディレクトリ構成

```
tests/
├── harness/
│   ├── mod.rs       # テストユーティリティ（render_to_string, create_test_*, composite state helpers）
│   └── fixtures.rs  # サンプルデータビルダー（metadata, table detail, query result）
└── render_snapshots/
    ├── main.rs      # エントリ: 共通 imports + mod 宣言
    ├── <category>.rs  # 画面カテゴリ別サブモジュール（ファイル名 = カテゴリ名）
    └── snapshots/   # 生成された .snap ファイル（insta が自動作成）
```

## 新しいシナリオの追加方法

1. `tests/render_snapshots/<category>.rs` にテスト関数を追加
2. `mise run test` を実行（新しいスナップショットで失敗する）
3. 生成された `.snap.new` をレビュー
4. `mise exec -- cargo insta accept` を実行

## カバレッジ基準

### モードカバレッジ義務

- すべての `InputMode` バリアントに最低1つのスナップショットテストが必要

### スナップショットテストを追加すべきとき

- **各 InputMode** — モードごとに最低1シナリオ
- **主要な UI 状態変更** — フォーカスペイン切り替え、メッセージ表示
- **境界条件** — 空の結果、初期ロード状態、エラー状態
- **テキスト入力コンポーネント** — カーソルが先頭、中間、末尾の3状態（最低3つ）

### 追加不要なケース

- **データバリエーション** — 同じ画面内での行数・列数の違い
- **網羅的な組み合わせ** — すべての状態の順列
- **一時的な状態** — 短時間のローディングインジケータ（ER 進捗表示のような永続的なものは除く）

## スナップショット更新ポリシー

### OK

- **意図的な UI 変更** — レイアウト、スタイリング、新機能
- **表示バグの修正で出力が変わる場合** — バグ修正後

### NG

- **リグレッションによるテスト失敗** — スナップショットではなくコードを修正する
- **意図しない変更** — まず diff を調査すること
