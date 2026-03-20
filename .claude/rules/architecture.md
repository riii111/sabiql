---
paths:
  - "**/src/**/*.rs"
---

# アーキテクチャルール

## レイヤ構造（ヘキサゴナル / Ports & Adapters）

```
src/
├── ui/          # プレゼンテーション層 + UI Adapters
├── app/         # アプリケーション層（State, Reducers, Ports）
├── infra/       # インフラストラクチャ Adapters
└── domain/      # ドメインモデル（純粋なデータ構造）
```

## 依存ルール

### OK
- `ui/` → `app/` → `domain/`
- `infra/adapters/` → `app/ports/`（trait を実装）
- `ui/adapters/` → `app/ports/`（trait を実装）

### NG

- `app/` → `ui/` — Renderer port 経由で行うこと
- `app/` → `infra/` — port 経由で行うこと（`app/ports/` に trait を定義し `infra/adapters/` で実装）
- `ui/` → `infra/`

## Ports & Adapters パターン

Port は `app/ports/` に定義された **trait** で、外部依存を抽象化する:

| Port | 用途 | Adapter の場所 |
|------|------|---------------|
| `MetadataProvider` | DBメタデータ取得 | `infra/adapters/` |
| `QueryExecutor` | SQL実行 | `infra/adapters/` |
| `ConfigWriter` | キャッシュディレクトリ | `infra/adapters/` |
| `ClipboardWriter` | クリップボード書き込み | `infra/adapters/` |
| `FolderOpener` | フォルダ表示 | `infra/adapters/` |
| `Renderer` | TUI描画 | `ui/adapters/` |

## 新規コードの配置先

| やりたいこと | 配置先 |
|-------------|--------|
| UIコンポーネント | `ui/`（詳細は ui-design.md） |
| ビジネスロジック | `app/`（reducer: reducer-structure.md, effect: effect-runner.md, keybind: interaction-contract.md） |
| 外部I/O | `app/ports/` に trait 定義 → `infra/adapters/` or `ui/adapters/` で実装 |
| ドメインモデル | `domain/` |

## 副作用境界

- `app/` は I/O 禁止。ファイルシステム、ネットワーク、プロセス起動は不可
- `domain/` は構造体とデータ変換のみ定義する
- 副作用が許可される場所: `infra/adapters/`, `ui/adapters/`, `main.rs` のみ
- Reducer は副作用を `Vec<Effect>` として返すこと。インラインで実行してはならない
- UI adapter（描画の抽象化）は `ui/adapters/` に置く（`infra/` ではない）
- Port による依存性逆転: app が必要なものを定義し、adapter が実装を提供
