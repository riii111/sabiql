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

**許可:**
- `ui/` → `app/` → `domain/`
- `infra/adapters/` → `app/ports/`（trait を実装）
- `ui/adapters/` → `app/ports/`（trait を実装）

## 禁止依存（違反不可）

- `app/` → `ui/` — 代わりに Renderer port を使う
- `app/` → `infra/` — MetadataProvider, ConfigWriter 等の port を使う
- `ui/` → `infra/`

app→infra の通信が必要な場合、`app/ports/` に port trait を定義し `infra/adapters/` で実装すること。

## Ports & Adapters パターン

Port は `app/ports/` に定義された **trait** で、外部依存を抽象化する:

| Port | 用途 | Adapter の場所 |
|------|------|---------------|
| `MetadataProvider` | DBメタデータ取得 | `infra/adapters/` |
| `QueryExecutor` | SQL実行 | `infra/adapters/` |
| `ConfigWriter` | キャッシュディレクトリ | `infra/adapters/` |
| `Renderer` | TUI描画 | `ui/adapters/` |

## 新規コードの配置先

| やりたいこと | 配置先 |
|-------------|--------|
| UIコンポーネント追加 | `ui/components/` |
| ビジネスロジック追加 | `app/`（純粋関数、I/Oなし） |
| 外部I/O追加 | `app/ports/` に port 定義 → `infra/adapters/` or `ui/adapters/` で実装 |
| DB固有のSQL・接続文字列ロジック追加 | `app/ports/` に port 定義 → `infra/adapters/` で実装 |
| ドメインモデル追加 | `domain/` |
| app層の純粋計算追加 | `app/`（例: `viewport.rs`, `ddl.rs`） |
| キーマッピング追加（simpleモード） | `app/keybindings/` の該当サブモジュールにエントリ追加; `keymap::resolve()` が自動処理 |
| キーマッピング追加（Normalモード） | `app/keybindings/normal.rs` + `mod.rs` に predicate fn + `handler.rs` で配線 |
| DB固有SQL・方言ロジック追加 | `infra/adapters/{postgres,mysql}/`（`app/ports/` には絶対に置かない） |

## 副作用境界（必須）

- `app/` は I/O 禁止。ファイルシステム、ネットワーク、プロセス起動は不可。
- `domain/` は純粋データのみ。副作用のあるメソッド禁止。
- 副作用が許可される場所: `infra/adapters/`, `ui/adapters/`, `main.rs` のみ
- Reducer は副作用を `Vec<Effect>` として返すこと。インラインで実行してはならない。

## 基本原則

1. **app/ は I/O フリー**: Reducer と状態ロジックに副作用なし。副作用はデータとして返す。
2. **Port による依存性逆転**: app が必要なものを定義し、adapter が実装を提供。
3. **UI adapter は UI の関心事**: 描画の抽象化は `ui/adapters/` に置く（`infra/` ではない）。
4. **Domain は純粋データ**: ドメインモデルにビジネスロジックは置かず、構造のみ。
