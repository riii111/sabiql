---
paths:
  - "**/src/**/*.rs"
---

# アーキテクチャルール

## レイヤ構造（ヘキサゴナル / Ports & Adapters）

```
src/
├── ui/          # プレゼンテーション層 + UI Adapters
├── app/         # アプリケーション層（Elm Architecture + Ports）
├── infra/       # インフラストラクチャ Adapters
└── domain/      # ドメインモデル（純粋なデータ構造）
```

## app/ 内部構造（Elm Architecture + role-first）

```
app/
├── model/       # Model: 純粋な状態と aggregate
│   ├── <feature>/    # feature 固有の状態
│   └── shared/       # feature 文脈のない再利用可能 state component
├── update/      # Update: Action 定義 + Reducer + 入力マッピング
│   ├── <feature>/    # feature 固有の reducer 群
│   └── input/        # keybindings, keymap, command 等
├── cmd/         # Cmd: Effect 定義 + Runner + Handler
│   └── <feature>/    # feature 固有の effect handler
├── policy/      # 状態を持たない判定・分類・変換ロジック
├── ports/       # 外部依存の trait 定義
└── services     # Update/Cmd に注入する依存束ね
```

### Elm Architecture との対応

| Elm | app/ の対応 | 説明 |
|-----|-----------|------|
| Model | `model/` | AppState + 各種状態 aggregate + shared component |
| Msg | `update/action.rs` | Action enum |
| Update | `update/reducer.rs` + `update/` 配下 | Reducer（Action → State 遷移 + Effect 返却） |
| Cmd | `cmd/effect.rs` + `cmd/` 配下 | Effect（副作用のデータ表現）+ EffectRunner + Handler |
| — | `policy/` | Elm にない追加層。状態を持たない純粋な判定・分類・変換 |
| — | `ports/` | ヘキサゴナルの追加要素。外部依存の trait 定義 |

### ディレクトリ分割ルール

- ディレクトリを切るのは **3ファイル以上**、または **2ファイルでも責務が明確に分かれる場合**
- 1ファイルのためのディレクトリは原則作らない

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

Port は `app/ports/` に定義された **trait** で、外部依存を抽象化する。
具体的な Port 一覧は `app/ports/` を参照のこと。

- Port による依存性逆転: app が必要なものを定義し、adapter が実装を提供
- `infra/adapters/` は infra 側の Port 実装、`ui/adapters/` は UI 側の Port 実装

## 新規コードの配置先

| やりたいこと | 配置先 |
|-------------|--------|
| UIコンポーネント | `ui/`（詳細は ui-design.md） |
| 状態 aggregate / shared component | `app/model/`（feature 固有 → `model/feature/`、汎用 → `model/shared/`） |
| Action / Reducer | `app/update/`（詳細は reducer-structure.md） |
| 入力マッピング（keybind, command） | `app/update/input/`（詳細は interaction-contract.md） |
| Effect / Handler | `app/cmd/`（詳細は effect-runner.md） |
| 状態を持たない判定・分類・変換 | `app/policy/` |
| 外部I/O の抽象 | `app/ports/` に trait 定義 → `infra/adapters/` or `ui/adapters/` で実装 |
| ドメインモデル | `domain/` |

## 副作用境界

- `app/` は I/O 禁止。ファイルシステム、ネットワーク、プロセス起動は不可
- `domain/` は構造体とデータ変換のみ定義する
- 副作用が許可される場所: `infra/adapters/`, `ui/adapters/`, `main.rs` のみ
- Reducer は副作用を `Vec<Effect>` として返すこと。インラインで実行してはならない
- UI adapter（描画の抽象化）は `ui/adapters/` に置く（`infra/` ではない）
