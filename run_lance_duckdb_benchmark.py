#!/usr/bin/env python3
import argparse
import os
import shlex
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent
DEFAULT_TABLE = "lance.main.search_benchmark_docs"
DEFAULT_TEXT_COLUMN = "text"
DEFAULT_COMMANDS = "TOP_100_COUNT TOP_100 COUNT"
DEFAULT_SMOKE_QUERY = "new york"


def sql_string(value):
    return "'" + value.replace("'", "''") + "'"


def detect_mysql_bin(explicit_path):
    candidates = []
    if explicit_path:
        candidates.append(explicit_path)
    env_path = os.environ.get("MYSQL_BIN")
    if env_path:
        candidates.append(env_path)
    path_bin = shutil.which("mysql")
    if path_bin:
        candidates.append(path_bin)
    fallback_bin = "/ssd/workspace4/mysql_install/SQLEngine/bin/mysql"
    if os.path.exists(fallback_bin):
        candidates.append(fallback_bin)

    for candidate in candidates:
        if candidate and os.path.exists(candidate):
            return candidate
    raise SystemExit(
        "mysql client not found; pass --mysql-bin or set MYSQL_BIN to a valid mysql executable"
    )


def print_step(title):
    print(f"\n==> {title}", flush=True)


def run_command(cmd, *, env=None, cwd=ROOT, capture_output=False):
    printable = shlex.join(cmd)
    print(f"+ {printable}", flush=True)
    completed = subprocess.run(
        cmd,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE if capture_output else None,
        stderr=subprocess.PIPE if capture_output else None,
        check=False,
    )
    if completed.returncode != 0:
        if capture_output:
            if completed.stdout:
                sys.stdout.write(completed.stdout)
            if completed.stderr:
                sys.stderr.write(completed.stderr)
        raise SystemExit(f"command failed with exit code {completed.returncode}: {printable}")
    return completed


def mysql_env(args):
    env = os.environ.copy()
    if args.password:
        env["MYSQL_PWD"] = args.password
    return env


def mysql_cmd(args, sql):
    return [
        args.mysql_bin,
        "--batch",
        "--raw",
        "--skip-column-names",
        "--connect-timeout=30",
        "-h",
        args.host,
        "-P",
        str(args.port),
        "-u",
        args.user,
        "-e",
        sql,
    ]


def mysql_query(args, sql):
    completed = run_command(mysql_cmd(args, sql), env=mysql_env(args), capture_output=True)
    return completed.stdout.strip()


def benchmark_env(args):
    env = os.environ.copy()
    env["PYTHONUNBUFFERED"] = "1"
    env["ENGINES"] = "lance-duckdb"
    env["LANCE_BENCH_HOST"] = args.host
    env["LANCE_BENCH_PORT"] = str(args.port)
    env["LANCE_BENCH_USER"] = args.user
    env["LANCE_BENCH_TABLE"] = args.table
    env["LANCE_BENCH_TEXT_COLUMN"] = args.text_column
    env["COMMANDS"] = args.bench_commands
    env["WARMUP_TIME"] = str(args.warmup_time)
    if args.password:
        env["LANCE_BENCH_PASSWORD"] = args.password
    return env


def ensure_corpus(args):
    corpus_path = ROOT / "corpus.json"
    if corpus_path.exists():
        return
    print_step("Corpus Not Found, Downloading")
    run_command(["make", "corpus"], env=benchmark_env(args))


def enable_local_infile(args):
    print_step("Enable local_infile")
    output = mysql_query(
        args,
        "set @@global.local_infile=1; SHOW GLOBAL VARIABLES LIKE 'local_infile';",
    )
    print(output, flush=True)
    if "local_infile" not in output or not any(token in output.upper() for token in ("ON", "1")):
        raise SystemExit(
            "failed to enable local_infile on server"
        )


def import_data(args):
    print_step("Import Benchmark Corpus")
    ensure_corpus(args)
    run_command(["make", "index"], env=benchmark_env(args))


def smoke_test(args):
    print_step("Smoke Test")
    local_infile_output = mysql_query(args, "SHOW GLOBAL VARIABLES LIKE 'local_infile';")
    count_output = mysql_query(args, f"SELECT count(*) FROM {args.table};")
    indexes_output = mysql_query(args, f"SHOW INDEXES ON {args.table};")
    sample_output = mysql_query(
        args,
        (
            "SELECT id FROM lance_fts("
            f"{sql_string(args.table)}, {sql_string(args.text_column)}, {sql_string(args.smoke_query)}, "
            f"k = {args.smoke_k}) LIMIT {args.smoke_k};"
        ),
    )

    doc_count = int(count_output.splitlines()[-1])
    if doc_count <= 0:
        raise SystemExit(f"smoke test failed: imported row count is {doc_count}")
    if not indexes_output.strip():
        raise SystemExit("smoke test failed: no index found on benchmark table")
    sample_rows = [line for line in sample_output.splitlines() if line.strip()]
    if not sample_rows:
        raise SystemExit("smoke test failed: lance_fts returned no rows")

    print(f"local_infile: {local_infile_output}", flush=True)
    print(f"doc_count: {doc_count}", flush=True)
    print("indexes:", flush=True)
    print(indexes_output, flush=True)
    print("sample_ids:", flush=True)
    print("\n".join(sample_rows[: args.smoke_k]), flush=True)


def run_full_benchmark(args):
    print_step("Full Benchmark")
    query_count = sum(1 for _ in (ROOT / "queries.txt").open())
    print(
        (
            "Warmup progress stays at 0.0% until one full sweep finishes. "
            f"This benchmark has {query_count} queries, and src/client.py only refreshes the bar "
            "after finishing a full pass."
        ),
        flush=True,
    )
    run_command(["make", "bench"], env=benchmark_env(args))
    print(f"results written to {ROOT / 'results.json'}", flush=True)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Enable local_infile, import the corpus, run a smoke test, then run the lance-duckdb benchmark.",
    )
    parser.add_argument("--host", required=True, help="SQLEngine host")
    parser.add_argument("--port", required=True, type=int, help="SQLEngine port")
    parser.add_argument("--user", default="root", help="MySQL user")
    parser.add_argument("--password", default=os.environ.get("MYSQL_PWD", ""), help="MySQL password")
    parser.add_argument("--mysql-bin", default="", help="Path to mysql client")
    parser.add_argument("--table", default=DEFAULT_TABLE, help="Benchmark table name")
    parser.add_argument("--text-column", default=DEFAULT_TEXT_COLUMN, help="Text column name")
    parser.add_argument("--smoke-query", default=DEFAULT_SMOKE_QUERY, help="Query used for smoke test")
    parser.add_argument("--smoke-k", default=5, type=int, help="Top-k used for smoke test")
    parser.add_argument(
        "--bench-commands",
        default=DEFAULT_COMMANDS,
        help="Commands passed to make bench through COMMANDS",
    )
    parser.add_argument(
        "--warmup-time",
        default=60,
        type=int,
        help="WARMUP_TIME passed to make bench",
    )
    parser.add_argument("--skip-index", action="store_true", help="Skip make index")
    parser.add_argument("--skip-bench", action="store_true", help="Skip make bench")
    return parser.parse_args()


def main():
    args = parse_args()
    args.mysql_bin = detect_mysql_bin(args.mysql_bin)

    enable_local_infile(args)
    if not args.skip_index:
        import_data(args)
    smoke_test(args)
    if not args.skip_bench:
        run_full_benchmark(args)


if __name__ == "__main__":
    main()
