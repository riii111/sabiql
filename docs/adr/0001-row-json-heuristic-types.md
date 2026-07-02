# Row JSON uses heuristic type inference

`QueryResult` currently stores every cell as a string because the adapter consumes `psql --csv` output. Adding proper database column types would require extending the query-execution pipeline and every adapter.

For the **Row JSON** modal — a read-only inspection view of the selected row — we decided to infer JSON types from cell strings instead. A cell that parses as a JSON scalar or object becomes that JSON value; otherwise it stays a string. This keeps the feature cheap and contained, at the cost of occasionally misclassifying string columns that happen to look like numbers or booleans.
