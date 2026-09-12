#!/usr/bin/env python3
"""Compare COUNT hit numbers across engines for a small query sample."""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAMPLE = [
    "the",
    "+griffith +observatory",
    '"griffith observatory"',
    "griffith observatory",
    "+climate policy",
    '+"the who" +uk',
]


def start(engine):
    cwd = ROOT / "engines" / engine
    return subprocess.Popen(
        ["make", "--no-print-directory", "serve"],
        cwd=cwd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        env={**dict(**{k: v for k, v in __import__("os").environ.items()}), "CORPUS": "/dev/shm/sbg/corpus.json"},
    )


def query(proc, q):
    proc.stdin.write(f"COUNT\t{q}\n".encode())
    proc.stdin.flush()
    line = proc.stdout.readline().decode().strip()
    if line == "UNSUPPORTED":
        return None
    return int(line)


def main():
    engines = sys.argv[1:] or [
        "lucene-10.3.0",
        "tantivy-0.25",
        "lance-f03a2783c24f",
        "lance-f03a2783c24f-mt",
    ]
    procs = {}
    try:
        for engine in engines:
            procs[engine] = start(engine)
        print("| query | " + " | ".join(engines) + " |")
        print("|---|" + "|".join(["---:"] * len(engines)) + "|")
        for q in SAMPLE:
            counts = []
            for engine in engines:
                try:
                    counts.append(str(query(procs[engine], q)))
                except Exception as exc:
                    counts.append(f"err:{exc}")
            print(f"| `{q}` | " + " | ".join(counts) + " |")
    finally:
        for proc in procs.values():
            try:
                proc.stdin.close()
                proc.terminate()
            except Exception:
                pass


if __name__ == "__main__":
    main()
