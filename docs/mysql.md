# MySQL support

Use this page to check whether your MySQL server, connection method, and expected workflow are supported.

sabiql supports Oracle MySQL server 8.4 LTS through the Oracle `mysql` CLI from the 8.4 series. MariaDB, Percona Server, TiDB, and other MySQL-compatible products are not formally supported.

## Connection requirements

MySQL connections use TCP. Unix sockets and Windows named pipes are not supported. When connecting to a remote server, install the local `mysql` CLI; a local MySQL server is not required.

Passphrase-protected TLS client keys are not supported. An unencrypted PEM private key can be used, but it weakens at-rest protection; store it with restrictive file permissions.

## SQL and session behavior

- **No persistent session state** — Temporary tables, session variables, and transactions do not carry over between operations. Multiple statements in one SQL submission share one session.
- **Supported SQL scope** — The SQL modal supports `SELECT`, `TABLE`, `SHOW`, and `DESCRIBE`; `INSERT`, `UPDATE`, and `DELETE`; `CREATE`/`ALTER`/`DROP`/`TRUNCATE TABLE`, single-pair `RENAME TABLE`, `CREATE`/`ALTER`/`DROP VIEW`, and `CREATE`/`DROP INDEX` (including `CREATE FULLTEXT INDEX`); and simple explicit transactions. `CREATE OR REPLACE VIEW` and trailing MySQL executable version comments on DDL are supported. Server administration, stored programs, prepared statements, XA statements, compound statements, table maintenance, `REPLACE`, `DO`, `HANDLER`, and `LOAD DATA`/`LOAD XML` are not supported, and `REPLACE` is also rejected by MySQL `EXPLAIN`.
- **No session or table-control SQL** — User `USE`, `SET`, `LOCK TABLES`, and `UNLOCK TABLES` statements are rejected. MySQL client commands such as `source`, `system`, `charset`, and backslash commands are rejected, as is `DELIMITER`.
- **SQL mode** — Connections using `NO_BACKSLASH_ESCAPES` or `ANSI_QUOTES` are rejected because those modes enable escape or identifier-quoting semantics that sabiql does not support.
- **Read-Only Mode is not a sandbox** — It blocks supported writes at the application and MySQL session levels, but it does not isolate external side effects from user-defined functions (UDFs).

## Data and editing limitations

- **NUL in text** — Query results cannot preserve NUL characters embedded in text values.
- **Binary values in arbitrary SQL** — Binary columns in table previews are handled as Blob values. Binary values returned by arbitrary SQL use the `0x...` text representation.
- **System databases** — `information_schema`, `mysql`, `performance_schema`, and `sys` are omitted from the MySQL Database Picker.
- **Grid editing scope** — Views, tables without a retrievable primary key, and generated columns are read-only in the grid. Invisible primary keys and generated invisible primary keys are used for row identity when available but are not shown as grid columns.
- **CSV representation** — NULL values are exported as empty fields. Blob values use uppercase hexadecimal strings; binary values from arbitrary SQL keep the `0x...` representation.
