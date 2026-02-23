---
description: Config migration rules - backward compatibility for connections.toml schema changes
alwaysApply: false
globs:
  - "**/src/infra/adapters/config_writer.rs"
  - "**/src/app/ports/config_writer.rs"
  - "**/src/infra/adapters/connection_store.rs"
---

# Config Migration Rules

## Backward Compatibility (MUST)

- Changes to `connections.toml` schema MUST NOT break existing config files
- New fields MUST have sensible defaults so old configs load without error
- Removed fields MUST be silently ignored during deserialization

## Schema Versioning

- If a breaking schema change is unavoidable, add a `version` field to the config
- Provide a migration path from the previous version
- Log a clear warning when migrating old configs automatically
