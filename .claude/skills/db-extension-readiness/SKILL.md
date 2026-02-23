---
name: db-extension-readiness
description: Review port traits and adapters for DB-agnostic design. Relevant when modifying app/ports/ traits, adding SQL generation, or discussing MySQL support.
user-invocable: false
---

# DB Extension Readiness Review

## When to Use

- After modifying any trait in `app/ports/`
- After adding SQL generation in `infra/adapters/postgres/`
- When planning MySQL adapter work

## Procedure

1. Scan all traits in `app/ports/` for PG-specific terminology or types
2. For each `infra/adapters/postgres/` method:
   a. Check if the SQL syntax is PG-only (e.g., `::type` casting, `$1` params)
   b. Verify the corresponding port trait method is dialect-neutral
3. Check if MySQL adapter module exists and compiles (even if tests are `#[ignore]`)
4. Verify no `use postgres::` or PG-specific imports in `app/` layer

## Output

- List of dialect-specific leaks into port layer
- MySQL adapter gaps (methods implemented in PG but not MySQL)

## Exit Criteria

- All port traits are dialect-neutral
- All PG-specific code is confined to `infra/adapters/postgres/`

## Escalation

- If a port trait fundamentally cannot be dialect-neutral, propose a design change with trait generics or associated types
