# lance-duckdb engine

This engine adapter lets `search-benchmark-game` run against a TDSQL SQLEngine
full-text standalone node (`--multimodal-mode`) backed by DuckDB and
`lance-duckdb`.

Start the full-text node first, then run the benchmark with this engine
explicitly selected:

```bash
python3 -m pip install pymysql
make corpus
LANCE_BENCH_PORT=23311 ENGINES=lance-duckdb make index
LANCE_BENCH_PORT=23311 ENGINES=lance-duckdb make bench
```

For the full standalone-node flow, you can also use the helper script in the
repository root. It verifies `local_infile`, optionally imports the corpus,
runs a small smoke test, then launches the full benchmark:

```bash
./run_lance_duckdb_benchmark.py --host 21.6.194.222 --port 23311
```

Useful flags:

- `--skip-index`: reuse an existing imported dataset.
- `--skip-bench`: only run the smoke test.
- `--warmup-time 1`: shorten warmup for quick validation.

Useful environment variables:

- `LANCE_BENCH_HOST`: MySQL host, defaults to `127.0.0.1`.
- `LANCE_BENCH_PORT`: MySQL port, defaults to `23311`.
- `LANCE_BENCH_USER`: MySQL user, defaults to `root`.
- `LANCE_BENCH_PASSWORD`: MySQL password, defaults to empty.
- `LANCE_BENCH_TABLE`: Lance table to create, defaults to
  `lance.main.search_benchmark_docs`.
- `LANCE_BENCH_DATASET`: Dataset argument passed to `lance_fts`, defaults to the
  indexed table name.
- `LANCE_BENCH_COUNT_K`: Override the `k` used for `COUNT` and `TOP_*_COUNT`.

The benchmark's count commands require total hit counts. `lance_fts` is a top-k
table function, so this adapter uses `k = indexed document count` by default for
count commands. That preserves semantics but makes count commands much more
expensive than pure top-k commands.
