# MySQL support

Use this page to check whether your MySQL server, connection method, and expected workflow are supported.

sabiql supports Oracle MySQL server 8.4 LTS through the Oracle `mysql` CLI from the 8.4 series. MariaDB, Percona Server, TiDB, and other MySQL-compatible products are not formally supported.

## Connection requirements

MySQL connections use TCP. Unix sockets and Windows named pipes are not supported. When connecting to a remote server, install the local `mysql` CLI; a local MySQL server is not required.

Passphrase-protected TLS client keys are not supported. An unencrypted PEM private key can be used, but it weakens at-rest protection; store it with restrictive file permissions.

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
