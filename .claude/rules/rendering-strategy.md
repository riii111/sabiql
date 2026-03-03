---
paths:
  - "**/src/app/render_schedule.rs"
  - "**/src/main.rs"
---

# レンダリング戦略

Ratatui は明示的な描画制御が必要。本アプリは**イベント駆動レンダリング**（固定FPSではない）を採用。

## レンダリングトリガー

| トリガー | レンダリングタイミング |
|---------|---------------------|
| 状態変更 | Reducer が `render_dirty = true` にセット → main loop が `Effect::Render` を追加 |
| アニメーション deadline | Spinner(150ms)、カーソル点滅(500ms)、メッセージタイムアウト、結果ハイライト |
| 無操作時 | 入力またはdeadlineまで無期限スリープ |

## アーキテクチャ分離

- `app/render_schedule.rs`: 次のdeadlineを計算する純粋関数（I/Oなし）
- `main.rs`: UI層が `tokio::select!` + `sleep_until(deadline)` を処理
