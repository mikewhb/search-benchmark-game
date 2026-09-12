#!/usr/bin/env python3
"""Print AVERAGE / P50 / P90 latency tables from results.json."""
import json
import statistics
import sys
from collections import defaultdict


def percentile(sorted_vals, p):
    if not sorted_vals:
        return None
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    idx = min(len(sorted_vals) - 1, int(round((p / 100.0) * (len(sorted_vals) - 1))))
    return sorted_vals[idx]


def best_us(durations):
    if not durations:
        return None
    return min(durations)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "results.json"
    with open(path) as f:
        payload = json.load(f)
    results = payload.get("results", payload)
    for command, engines in results.items():
        print(f"\n## {command}")
        rows = []
        for engine, queries in engines.items():
            bests = [best_us(q.get("duration") or []) for q in queries]
            bests = [v for v in bests if v is not None]
            if not bests:
                continue
            bests.sort()
            rows.append(
                (
                    engine,
                    statistics.mean(bests),
                    percentile(bests, 50),
                    percentile(bests, 90),
                    percentile(bests, 99),
                    len(bests),
                )
            )
        rows.sort(key=lambda r: r[1])
        print("| Engine | AVERAGE μs | P50 μs | P90 μs | P99 μs | queries |")
        print("|---|---:|---:|---:|---:|---:|")
        for engine, avg, p50, p90, p99, n in rows:
            print(
                f"| {engine} | {avg:,.0f} | {p50:,.0f} | {p90:,.0f} | {p99:,.0f} | {n} |"
            )


if __name__ == "__main__":
    main()
