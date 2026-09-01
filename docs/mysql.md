# MySQL support

Use this page to check whether your MySQL server, connection method, and expected workflow are supported.

sabiql requires the Oracle MySQL `mysql` CLI from the 8.4 series. The server may be Oracle MySQL 5.7, 8.0, or 8.4; 8.4 is the continuously validated server version, while other Oracle server versions are not fully guaranteed. MariaDB, Percona Server, TiDB, Vitess, Aurora, and other MySQL-compatible products are not supported; the named products are rejected separately when `VERSION()` identifies them.

## Connection requirements

MySQL connections can use TCP, a Unix socket file on Unix-like systems, or a Windows named pipe. Select the transport explicitly and provide its socket or pipe path when required. Shared-memory transport and automatic transport fallback are not supported. When connecting to a remote server, install the local `mysql` CLI; a local MySQL server is not required.

Passphrase-protected TLS client keys are not supported. An unencrypted PEM private key can be used, but it weakens at-rest protection; store it with restrictive file permissions.

## Version differences

The Explorer, Inspector, and query-result panes select metadata and empty-result queries from the server version reported by `VERSION()`:

- MySQL 5.7 uses `GENERATION_EXPRESSION` where available, but the legacy `STATISTICS` shape without functional-index expressions or index visibility. Empty `SELECT` column names use a derived-table fallback because common table expressions are unavailable.
- MySQL 8.0 uses `GENERATION_EXPRESSION` and `IS_VISIBLE` where available. Functional-index expressions are selected from MySQL 8.0.13 onward, and common table expressions from 8.0.1 onward.
- MySQL 8.4 uses the complete metadata shape, including functional-index expressions and invisible-index state.

The Oracle MySQL 8.4 CLI remains required for local execution and XML result parsing. These version differences do not expand support for version-specific `EXPLAIN`, `EXPLAIN ANALYZE`, or `TABLE` behavior.

## SQL and session behavior

- **No persistent session state** — Temporary tables, session variables, and transactions do not carry over between operations. Multiple statements in one SQL submission share one session.
- **Supported SQL scope** — The SQL modal supports `SELECT`, `TABLE`, `SHOW`, and `DESCRIBE`; `INSERT`, `REPLACE`, `UPDATE`, and `DELETE`; `CREATE`/`ALTER`/`DROP`/`TRUNCATE TABLE`, same-database single-pair `RENAME TABLE`, `CREATE`/`ALTER`/`DROP VIEW`, and `CREATE [UNIQUE | FULLTEXT]`/`DROP INDEX` (`SPATIAL` indexes are not supported); and simple explicit transactions. `CREATE OR REPLACE VIEW` and `ALTER VIEW` accept MySQL's `ALGORITHM`, `DEFINER`, and `SQL SECURITY` modifiers. Selected safe trailing MySQL executable version-comment clauses are supported, such as `DEFAULT CHARSET` on table DDL and `RESTRICT`/`CASCADE` on `DROP TABLE`; unsupported or ambiguous versioned comments are rejected. Server administration, stored programs, prepared statements, XA statements, compound statements, table maintenance, `DO`, `HANDLER`, and `LOAD DATA`/`LOAD XML` are not supported. `REPLACE` is supported by the SQL modal and regular MySQL `EXPLAIN`, but not `EXPLAIN ANALYZE`.
- **No session or table-control SQL** — User `USE`, `SET`, `LOCK TABLES`, and `UNLOCK TABLES` statements are rejected. MySQL client commands such as `source`, `system`, `charset`, and backslash commands are rejected, as is `DELIMITER`.
- **SQL mode** — Connections using `NO_BACKSLASH_ESCAPES` or `ANSI_QUOTES` are rejected because those modes enable escape or identifier-quoting semantics that sabiql does not support.
- **Read-Only Mode is not a sandbox** — It blocks supported writes at the application and MySQL session levels, but it does not isolate external side effects from user-defined functions (UDFs).

## Data and editing limitations

- **NUL in text** — Query results cannot preserve NUL characters embedded in text values.
- **Binary values in arbitrary SQL** — Binary columns in table previews are handled as Blob values. Binary values returned by arbitrary SQL use the `0x...` text representation.
- **Grid editing scope** — Views, tables without a retrievable primary key, and generated columns are read-only in the grid. Invisible primary keys and generated invisible primary keys are used for row identity when available but are not shown as grid columns.
- **CSV representation** — NULL values are exported as empty fields. Blob values use uppercase hexadecimal strings; binary values from arbitrary SQL keep the `0x...` representation.
- **CSV size limits** — A decoded CSV field and a decoded CSV row are each limited to 16 MiB (16,777,216 bytes). Query and CSV-export `mysql` processes set the separate protocol packet limit to 32 MiB (33,554,432 bytes); this is the client/server transport limit, not the decoded CSV limit. A field or row above its decoded limit fails with the limit in the error, and the temporary CSV is not published. sabiql does not spill oversized rows to a temporary file or increase these limits automatically.
