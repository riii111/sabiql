# Observed synthetic results

Adapter base: `a5e22c7a1e6c89c091d0e3ef267c17dddaaa208d`. See [protocol and limitations](README.md).

100 tables, fresh-harness, five observations per cell. Times are milliseconds; p95 is the maximum of five. The corrected sequential run supplies db_selection. Other rows use the initial sweep. Process and SQL counts below were identical across all five observations in each shown cell.

| Adapter boundary | Processes | SQL | 0 ms median | 50 ms median | 200 ms median / p95 |
|---|---:|---:|---:|---:|---:|
| startup | 2 | 1 | 89.7 | 152.6 | 332.2 / 346.8 |
| db_selection | 4 | 11 | 397.7 | 838.2 | 1873.5 / 1950.4 |
| metadata_reload | 1 | 5 | 159.4 | 358.9 | 799.2 / 810.5 |
| table_selection | 2 | 18 | 49.7 | 583.9 | 1953.3 / 1963.9 |
| inspector | 1 | 11 | 42.3 | 589.7 | 1947.2 / 1955.4 |
| preview | 1 | 7 | 42.3 | 362.1 | 1105.3 / 1118.6 |
| page | 1 | 7 | 42.1 | 367.1 | 1099.6 / 1108.2 |
| completion | 1 | 8 | 158.5 | 526.4 | 1428.4 / 1431.7 |
| er_metadata | 1 | 8 | 166.9 | 555.1 | 1460.6 / 1470.1 |

There are 810 accepted normal-case samples (720 from the initial sweep plus 90 corrected DB-selection samples). An additional 90 initial DB-selection observations are retained but excluded. All accepted calls succeeded; no logged fake PID was present when checked after its pair. Maximum concurrency is null because forced exits invalidate the log-based counter. Generated XML bytes are not delivered/network bytes.

| Catalog size | Reload generated XML bytes | ER metadata generated XML bytes |
|---|---:|---:|
| 10 | 4269 | 8483 |
| 100 | 37569 | 77603 |
| 1000 | 370569 | 768803 |

The measurements expose sensitivity to injected response delay and increasing catalog payload; they do not isolate real connection establishment, actual SQL time, parsing cost, or drawing. No production optimization or decision to retain the current structure follows from these synthetic samples alone. A follow-up can measure same-operation session preparation with the same fixture, then compare accepted semantics and latency; CF-02/07 post-merge measurements must be separately labeled.

Fault observations (two calls each): probe timeout 11,050 / 11,075 ms; metadata timeout 31,005 / 31,010 ms; abort/join after 250 ms completed in about 254 ms per call. The listener accepted two synthetic connections per case. No fake PID remained 500 ms after each pair; raw peer EOF events and timestamps are retained. This does not establish immediate cleanup or DB-side cancellation.

Verification:

```sh
python3 scripts/cf14/verify.py docs/measurements/cf14/baseline-a5e22c7a
python3 scripts/cf14/verify.py docs/measurements/cf14/db-selection-a5e22c7a
```
