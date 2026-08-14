# SQLite support

sabiql supports browsing, editing, and ad-hoc SQL on existing, regular SQLite database files. It requires `sqlite3` version 3.41.1 or later.

## Opening a database

Pass a database file path or a `sqlite://` DSN:

```bash
sabiql /path/to/app.db
sabiql /path/to/History
sabiql sqlite:///path/to/app.db
```

The `sqlite://` form treats everything after the prefix as a raw path and does not percent-decode URI escapes.

## Limitations

- **Existing file paths only** — In-memory databases (`:memory:`) and SQLite URI filenames (`file:...`) are not supported. Opening a path that does not exist does not create a database.
- **Main database only** — Attached and temporary databases are not browsed as separate namespaces.
- **Session state is per operation** — sabiql starts a new `sqlite3` process for each operation. Statements in one SQL modal submission share that process, but `TEMP` tables/views and connection-local `PRAGMA` settings do not carry over to the next SQL execution. Persistent changes to the main database remain available.
- **Grid editing requires a declared primary key** — Regular tables with a declared `PRIMARY KEY` support grid editing. Tables without a declared primary key, views, and virtual tables remain browsable but are read-only targets in the grid.
- **Query plans** — SQLite shows `EXPLAIN QUERY PLAN` in the Plan tab. Plan comparison and `EXPLAIN ANALYZE` are PostgreSQL-only.
- **No ER diagrams** — Graphviz export requires PostgreSQL metadata.
- **No JSON tree view** — Structured JSON editing is PostgreSQL-only.

## Query safety

sabiql accepts SQL-only input in the SQL modal. sqlite3 dot commands are rejected instead of being passed to the underlying client.

Every sqlite3 command runs in safe mode, preventing SQL from accessing files, extensions, or databases outside the selected database file. Operations such as `ATTACH` and `VACUUM` are unavailable because they require capabilities disabled by safe mode.

sabiql wraps transactional writes, including persistent PRAGMAs such as `user_version`, unless the input contains transaction control or a session-side-effect or transaction-incompatible statement such as `PRAGMA journal_mode`, `PRAGMA foreign_keys`, or `PRAGMA synchronous`.
