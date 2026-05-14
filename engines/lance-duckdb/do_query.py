#!/usr/bin/env python3
import os
import re
import sys

from common import connect, get_text_column, read_metadata


TOP_COMMAND = re.compile(r"^TOP_(\d+)(_COUNT)?$")


def count_k(metadata):
    override = os.environ.get("LANCE_BENCH_COUNT_K")
    if override:
        return max(1, int(override))
    return max(1, int(metadata.get("doc_count", 1)))


def parse_command(command, metadata):
    if command in ("COUNT", "UNOPTIMIZED_COUNT"):
        return True, count_k(metadata)

    match = TOP_COMMAND.match(command)
    if not match:
        return None

    top_k = int(match.group(1))
    if match.group(2):
        return True, count_k(metadata)
    return False, top_k


def run_query(cursor, dataset, text_column, command, query, metadata):
    parsed = parse_command(command, metadata)
    if parsed is None:
        return None

    needs_count, k = parsed
    if needs_count:
        cursor.execute(
            "SELECT count(*) FROM lance_fts(%s, %s, %s, k = %s)",
            (dataset, text_column, query, k),
        )
        return int(cursor.fetchone()[0])

    cursor.execute(
        "SELECT id FROM lance_fts(%s, %s, %s, k = %s) LIMIT %s",
        (dataset, text_column, query, k, k),
    )
    cursor.fetchall()
    return 1


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: do_query.py <index-dir>")

    metadata = read_metadata(sys.argv[1])
    dataset = os.environ.get("LANCE_BENCH_DATASET", metadata["dataset"])
    text_column = get_text_column(metadata.get("text_column", "text"))

    with connect(local_infile=False) as conn:
        with conn.cursor() as cursor:
            for line in sys.stdin:
                fields = line.rstrip("\n").split("\t", 1)
                if len(fields) != 2:
                    print("UNSUPPORTED", flush=True)
                    continue

                command, query = fields
                try:
                    result = run_query(cursor, dataset, text_column, command, query, metadata)
                except Exception as exc:
                    print(f"lance-duckdb query failed: {exc}", file=sys.stderr, flush=True)
                    result = None

                if result is None:
                    print("UNSUPPORTED", flush=True)
                else:
                    print(result, flush=True)


if __name__ == "__main__":
    main()
