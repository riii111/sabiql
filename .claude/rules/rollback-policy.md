---
paths:
  - "**/src/infra/adapters/postgres/psql/parser.rs"
---

# Command tag aggregation rollback policy

## psql の制約

psql の completion tag には savepoint 名が含まれない（`SAVEPOINT` / `RELEASE` / `ROLLBACK` はすべて bare）。
そのため `discard_rolled_back` は depth ベースの近似で動作する。

## 設計判断: false-positive-over-missed

曖昧なケースでは **不要な refresh を許容し、必要な refresh を見逃さない** 方向に倒す。
