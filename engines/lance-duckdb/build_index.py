#!/usr/bin/env python3
import csv
import json
import os
import shutil
import sys
from pathlib import Path

from common import (
    connect,
    get_index_name,
    get_load_target,
    get_table_name,
    get_text_column,
    write_metadata,
)


def write_tsv(tsv_path):
    doc_count = 0
    with tsv_path.open("w", encoding="utf-8", newline="") as tsv_file:
        writer = csv.writer(
            tsv_file,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            escapechar="\\",
        )
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            doc = json.loads(line)
            writer.writerow([doc["id"], doc["text"]])
            doc_count += 1
            if doc_count % 100000 == 0:
                print(f"Indexed input rows: {doc_count}", file=sys.stderr, flush=True)
    return doc_count


def recreate_table(cursor, table_name, text_column):
    if os.environ.get("LANCE_BENCH_DROP_TABLE", "1") != "0":
        cursor.execute(f"DROP TABLE IF EXISTS {table_name}")
    cursor.execute(f"CREATE TABLE {table_name} (id VARCHAR, {text_column} VARCHAR)")


def load_tsv(cursor, table_name, tsv_path):
    schema_name, load_table_name = get_load_target(table_name)
    if schema_name:
        cursor.execute(f"USE {schema_name}")
    escaped_path = str(tsv_path.resolve())
    cursor.execute(
        f"""
        LOAD DATA LOCAL INFILE %s
        INTO TABLE {load_table_name}
        FIELDS TERMINATED BY '\t'
        LINES TERMINATED BY '\n'
        """,
        (escaped_path,),
    )


def create_inverted_index(cursor, table_name, text_column):
    index_name = get_index_name()
    cursor.execute(f"CREATE INDEX {index_name} ON {table_name} ({text_column}) USING INVERTED")


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: build_index.py <index-dir>")

    index_dir = Path(sys.argv[1])
    if index_dir.exists():
        shutil.rmtree(index_dir)
    index_dir.mkdir(parents=True)

    table_name = get_table_name()
    text_column = get_text_column()
    tsv_path = index_dir / "corpus.tsv"
    doc_count = write_tsv(tsv_path)

    with connect(local_infile=True) as conn:
        with conn.cursor() as cursor:
            recreate_table(cursor, table_name, text_column)
            load_tsv(cursor, table_name, tsv_path)
            create_inverted_index(cursor, table_name, text_column)

    write_metadata(
        index_dir,
        {
            "doc_count": doc_count,
            "dataset": table_name,
            "table": table_name,
            "text_column": text_column,
        },
    )
    print(f"Loaded {doc_count} rows into {table_name}", file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
