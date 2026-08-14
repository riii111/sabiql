# MySQL support

sabiql supports Oracle MySQL server 8.4 LTS through the Oracle `mysql` CLI from the 8.4 series. MariaDB, Percona Server, TiDB, and other MySQL-compatible products are not formally supported.

## Connecting

MySQL connections use TCP. Unix sockets and Windows named pipes are not supported. When connecting to a remote server, install the local `mysql` CLI; a local MySQL server is not required.

In the connection form, set **Type** to `MySQL`, then enter the host, port, user, password, optional initial database, and TLS mode. If no initial database is set, the database picker opens after the connection succeeds. You can also use the picker later to switch databases without reconnecting.

Passphrase-protected TLS client keys are not supported. An unencrypted PEM private key can be used, but it weakens at-rest protection; store it with restrictive file permissions.

## SQL and session behavior

- **No persistent session state** — sabiql starts a new `mysql` process for each operation. Temporary tables, session variables, and transactions do not carry over to the next SQL submission. Multiple statements in one submission share one process and session.
- **Supported SQL scope** — The SQL modal supports reads such as `SELECT`, `TABLE`, `SHOW`, and `DESCRIBE`, DML (`INSERT`, `UPDATE`, and `DELETE`), table/view/index DDL, and simple explicit transactions. Database/account/role/replication/server/plugin/routine/event/trigger administration statements are not supported. `REPLACE`, `CALL`, `DO`, `HANDLER`, `LOAD DATA`/`LOAD XML`, table-maintenance statements, prepared statements, XA statements, and compound statements are also rejected.
- **No session or table-control SQL** — User `USE`, `SET`, `LOCK TABLES`, and `UNLOCK TABLES` statements are rejected. MySQL client commands such as `source`, `system`, `charset`, and backslash commands are rejected, as is `DELIMITER`.
- **SQL mode** — Connections using `NO_BACKSLASH_ESCAPES` or `ANSI_QUOTES` are rejected because those modes enable escape or identifier-quoting semantics that sabiql does not support.
- **Read-Only Mode is not a sandbox** — It blocks supported writes at the application and MySQL session levels, but it does not isolate external side effects from user-defined functions (UDFs).

## Data and editing limitations

- **NUL in text** — The MySQL CLI XML output cannot preserve NUL characters embedded in text values.
- **Binary values in arbitrary SQL** — Arbitrary SQL results do not include reliable column type metadata, so binary values are kept as the CLI's `0x...` text representation. Known binary columns in table previews are handled as Blob values.
- **System databases** — `information_schema`, `mysql`, `performance_schema`, and `sys` are omitted from the MySQL Database Picker.
- **Grid editing scope** — Views, tables without a retrievable primary key, and generated columns are read-only in the grid. Invisible primary keys and generated invisible primary keys are used for row identity when available but are not shown as grid columns.
- **CSV representation** — NULL values are exported as empty fields. Known cached Blob values use uppercase hexadecimal strings; binary values from type-unknown arbitrary SQL keep the CLI's `0x...` representation.
