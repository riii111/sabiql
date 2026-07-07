# Row Detail uses heuristic type inference for JSON copy

`QueryResult` currently stores every cell as a string because the adapter consumes `psql --csv` output. Adding proper database column types would require extending the query-execution pipeline and every adapter.

The **Row Detail** modal displays the selected row as plain text — one column name followed by its raw cell value. For the separate "Copy JSON" action, we decided to infer JSON types from cell strings and produce a JSON object with column names as keys. A cell that parses as a JSON scalar, object, or array becomes that JSON value; otherwise it stays a string. This keeps the feature cheap and contained, at the cost of occasionally misclassifying string columns that happen to look like numbers or booleans.
