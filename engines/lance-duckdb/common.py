import json
import os
import re
from pathlib import Path

import pymysql


IDENTIFIER = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*){0,2}$")


def get_table_name():
    table_name = os.environ.get("LANCE_BENCH_TABLE", "lance.main.search_benchmark_docs")
    if not IDENTIFIER.match(table_name):
        raise ValueError(f"invalid LANCE_BENCH_TABLE: {table_name!r}")
    return table_name


def get_text_column(default="text"):
    text_column = os.environ.get("LANCE_BENCH_TEXT_COLUMN", default)
    if not IDENTIFIER.match(text_column) or "." in text_column:
        raise ValueError(f"invalid LANCE_BENCH_TEXT_COLUMN: {text_column!r}")
    return text_column


def get_index_name():
    index_name = os.environ.get("LANCE_BENCH_INDEX_NAME", "search_benchmark_text_idx")
    if not IDENTIFIER.match(index_name) or "." in index_name:
        raise ValueError(f"invalid LANCE_BENCH_INDEX_NAME: {index_name!r}")
    return index_name


def get_load_target(table_name):
    parts = table_name.split(".")
    if len(parts) == 3:
        _, schema_name, short_table_name = parts
        return schema_name, short_table_name
    if len(parts) == 2:
        schema_name, short_table_name = parts
        return schema_name, short_table_name
    return None, parts[0]


def connect(local_infile=False):
    kwargs = {
        "host": os.environ.get("LANCE_BENCH_HOST", os.environ.get("MYSQL_HOST", "127.0.0.1")),
        "port": int(os.environ.get("LANCE_BENCH_PORT", os.environ.get("MYSQL_TCP_PORT", "23311"))),
        "user": os.environ.get("LANCE_BENCH_USER", os.environ.get("MYSQL_USER", "root")),
        "password": os.environ.get("LANCE_BENCH_PASSWORD", os.environ.get("MYSQL_PWD", "")),
        "charset": "utf8mb4",
        "autocommit": True,
        "local_infile": local_infile,
        "read_timeout": int(os.environ.get("LANCE_BENCH_READ_TIMEOUT", "3600")),
        "write_timeout": int(os.environ.get("LANCE_BENCH_WRITE_TIMEOUT", "3600")),
    }
    database = os.environ.get("LANCE_BENCH_DATABASE")
    if database:
        kwargs["database"] = database
    return pymysql.connect(**kwargs)


def metadata_path(index_dir):
    return Path(index_dir) / "metadata.json"


def write_metadata(index_dir, metadata):
    metadata_path(index_dir).write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n")


def read_metadata(index_dir):
    with metadata_path(index_dir).open() as metadata_file:
        return json.load(metadata_file)
