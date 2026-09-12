# Lance FTS vs Lucene 10.3.0 vs Tantivy 0.25

Local experiment on `search-benchmark-game`, comparing **this machine's**
`lance-distributed-foyer` commit `f03a2783c24f` with the harness's Lucene and
Tantivy engines. Not an official Search Benchmark Game result.

A full Chinese write-up (setup, numbers, RCA, community direction) is
`lance-distributed-foyer/docs/fts-bench-lance-lucene-tantivy.md`.

Branch: `feat/lance-f03a2783c24f`

## What the harness measures

Each engine implements two programs:

- `index`: stdin JSONL `{id, text}` → write `idx/`
- `serve`: stdin `COMMAND<TAB>query` → stdout one integer (hit count, or `1` for top-k)

`src/client.py` warms up at least 60 seconds, then runs 10 iterations and keeps
the best time per query. Queries are fed to a long-lived process, one thread at
a time.

**Corpus** (`make corpus`): English Wikipedia dump
`wiki-articles.json.bz2`, transformed by `corpus_transform.py` (keep url+body,
lowercase, non-letters → space). This run: **5,032,104** docs, **7.7G**
`corpus.json` on tmpfs.

**Queries** (`queries.txt`, 943 lines, AOL-derived):

| Shape | Count | Lucene meaning | Example |
|---|---|---|---|
| union | 301 | OR | `griffith observatory` |
| intersection | 300 | MUST+MUST | `+griffith +observatory` |
| phrase | 300 | phrase | `"griffith observatory"` |
| intersection_union | 40 | MUST + SHOULD | `+climate policy` |
| term | 1 | single term | `the` |
| two-phase | 1 | MUST phrase + MUST term | `+"the who" +uk` |

This run uses `TOP_10 TOP_100 COUNT TOP_10_COUNT TOP_100_COUNT`.

Official Lucene settings in this repo: `StandardAnalyzer` + empty stopwords,
`BM25(k1=0.9, b=0.4)`, `forceMerge(1)`, query cache off, text not stored.
Tantivy 0.25: `TEXT` with positions, no stemming, merge to one segment.

## Why existing Lance branches were not reused

| Branch | What it actually benches | Why it is not this experiment |
|---|---|---|
| `feat_lance_duckdb` | TDSQL `lance_fts()` | SQLEngine / DuckDB, not this Lance tree |
| `lancedb_bench` | crates.io `lancedb 0.25` | Old wrapper; AND was wrong in Python; COUNT used `limit=1e6`; `select(id)` takes stored columns |

Those latency numbers are not a baseline for `f03a2783c24f`.

## Locked decisions

- Keep source text in the Lance table; search does **not** take `text` or `id`.
- A1 `lance-f03a2783c24f`: one FTS partition (`LANCE_FTS_NUM_SHARDS=1`, large
  `LANCE_FTS_PARTITION_SIZE`), Tokio current-thread, `LANCE_FTS_SEARCH_CHUNK=1`.
- A2 `lance-f03a2783c24f-mt`: product-default partitions and `SEARCH_CHUNK=16`,
  multi-thread Tokio.
- B3: run both COUNT and TOP-k. Lance COUNT scores every match (no count-only
  collector). Lucene `searcher.count()` need not score.
- Data on `/dev/shm/sbg/` only. No Wikipedia / idx on `/data`.
- Do not change Lance product code (BM25 stays k1=1.2, b=0.75).
- `OptimizeOptions::merge(1)` merges delta index segments, not FTS partitions.
  A1 is built with one shard instead of copy+merge.

## Fairness table

| Setting | Lucene 10.3.0 | Tantivy 0.25 | Lance A1 | Lance A2 |
|---|---|---|---|---|
| stem / stopwords | off / empty | no stem | explicit off | same |
| tokenizer | StandardAnalyzer | default | `simple` | `simple` |
| positions | yes | yes | `with_position=true` | same |
| posting block | 128 | 128 | 128 | 128 |
| BM25 | 0.9 / 0.4 | 1.2 / 0.75 | 1.2 / 0.75 hardcoded | same |
| segments | forceMerge(1) | 1 segment | 1 FTS partition | 4 partitions |
| query threads | 1 | 1 | 1 | default parallel |
| take stored text | no | no | no | no |
| COUNT | `count()`, may skip scoring | Count collector | scored full match | same |

Query mapping (closed subset of Lucene query language, not a full parser):

- `"foo bar"` → PhraseQuery
- `+foo +bar` → MatchQuery AND
- `foo bar` → MatchQuery OR
- `foo` → MatchQuery
- `+climate policy` → Boolean MUST + SHOULD
- `+"the who" +uk` → Boolean MUST Phrase + MUST term

Correctness for this bench is **matching-document count**, not top-k id
identity. BM25 parameters differ, so ranked sets may differ from Lucene.

## Reproduce

```bash
export JAVA_HOME=/tmp/jdk-21.0.8+9   # Lucene 10.3 needs 21
export CORPUS=/dev/shm/sbg/corpus.json
export SBG_ROOT=/dev/shm/sbg
export COMMANDS='TOP_10 TOP_100 COUNT TOP_10_COUNT TOP_100_COUNT'
export ENGINES='tantivy-0.25 lucene-10.3.0 lance-f03a2783c24f-mt lance-f03a2783c24f'
export WARMUP_TIME=60

make corpus
make compile
make index
make bench
python3 scripts/summarize_results.py results.json
python3 scripts/check_counts.py
```

Lance binaries path-depend `/data/arrow/code/lance-distributed-foyer` @
`f03a2783c24f`. `CARGO_TARGET_DIR` for Lance is `/tmp/sbg-target` (local disk,
not `/data`).

## Machine (this run)

- Kernel: linux 6.6.88-32.tl4.x86_64
- RAM: 247 GiB; `/dev/shm` 124G tmpfs; `/data` is cloud disk and was not used
  for corpus or indexes
- rustc 1.97.0; Temurin 21.0.8 for Lucene
- Lance: `f03a2783c24f` (`12.0.0-beta.6`)
- Wall clock for `client.py`: **3h 26m** (`BENCH_EXIT:0`, 2026-09-03 04:20–07:45 UTC)

Index / data sizes on tmpfs (`df /dev/shm`: 26G / 124G used after the run).
Lance keeps the Wikipedia `text` column in the table (`data/`, 3.3G); that is
**not** the FTS index. The inverted index lives under `_indices/` and is what
to compare with Lucene / Tantivy.

| | Lucene 10.3.0 | Tantivy 0.25 | Lance A1 FTS | Lance A2 FTS |
|---|---:|---:|---:|---:|
| inverted index | 2.59 GiB | 2.80 GiB | 2.97 GiB | 3.20 GiB |
| vs Lucene | 1.00× | 1.08× | 1.15× | 1.24× |
| postings + positions | 2.50 GiB (`.doc`+`.pos`) | 2.70 GiB (`.idx`+`.pos`) | 2.91 GiB (`invert`) | 3.12 GiB (`invert`) |
| term dict | 38 MiB (`.tim`) | 45 MiB (`.term`) | 29 MiB (`tokens`) | 52 MiB (`tokens`) |
| norms / per-doc | 4.8 MiB (`.nvd`) | 4.8 MiB (`.fieldnorm`) | 28 MiB (`docs`) | 28 MiB (`docs`) |
| stored text in this dir | no (`Store.NO`; `.fdt` 40 MiB) | `id` only (`.store` 51 MiB) | table `data/` 3.29 GiB, not in FTS | same 3.29 GiB |

Apples-to-apples FTS-only: Lance A1 is **~15% larger** than Lucene and **~6%
larger** than Tantivy. The 6.3G / 6.5G dataset directories include a second
copy of the corpus as Lance table files; search does not take that column
(`empty_project` + `with_row_id` + `fast_search`).

## Hit-count sample

Taken from this run's `results.json` `COUNT` field. **All 943 queries** have
the same count on Lucene, Tantivy, Lance A1, and Lance A2 (0 mismatches).

| query | lucene-10.3.0 | tantivy-0.25 | lance-f03a2783c24f | lance-f03a2783c24f-mt |
|---|---:|---:|---:|---:|
| `the` | 4168066 | 4168066 | 4168066 | 4168066 |
| `+griffith +observatory` | 79 | 79 | 79 | 79 |
| `"griffith observatory"` | 57 | 57 | 57 | 57 |
| `griffith observatory` | 15456 | 15456 | 15456 | 15456 |
| `+climate policy` | 42009 | 42009 | 42009 | 42009 |
| `+"the who" +uk` | 660 | 660 | 660 | 660 |

Top-k **ids** need not match: Lucene BM25 is 0.9/0.4, Lance and Tantivy use
1.2/0.75.

## Latency

Times are the best of 10 runs, in microseconds, as reported by `src/client.py`.
AVERAGE is the mean of those 943 per-query best times. P50 / P90 / P99 are
percentiles of the same distribution.

`the` (4.17M hits) dominates AVERAGE on every COUNT-like command for Lance,
because Lance scores every match. P50 is the better "typical query" number.

### TOP_10

| Engine | AVERAGE μs | P50 μs | P90 μs | P99 μs | queries |
|---|---:|---:|---:|---:|---:|
| lucene-10.3.0 | 1,486 | 644 | 2,605 | 10,187 | 943 |
| tantivy-0.25 | 1,915 | 653 | 3,016 | 14,561 | 943 |
| lance-f03a2783c24f | 3,795 | 1,368 | 6,959 | 53,630 | 943 |
| lance-f03a2783c24f-mt | 4,011 | 1,491 | 7,247 | 53,347 | 943 |

A1 vs Lucene: AVERAGE **2.55×**, P50 **2.12×**. A1 is slightly faster than A2
on this single-query-at-a-time harness (extra partitions do not help).

### TOP_100

| Engine | AVERAGE μs | P50 μs | P90 μs | P99 μs | queries |
|---|---:|---:|---:|---:|---:|
| lucene-10.3.0 | 2,029 | 907 | 3,552 | 14,114 | 943 |
| tantivy-0.25 | 2,237 | 780 | 3,646 | 19,510 | 943 |
| lance-f03a2783c24f | 4,705 | 1,782 | 8,760 | 54,767 | 943 |
| lance-f03a2783c24f-mt | 4,960 | 1,939 | 9,206 | 56,354 | 943 |

A1 vs Lucene: AVERAGE **2.32×**, P50 **1.96×**.

### COUNT

| Engine | AVERAGE μs | P50 μs | P90 μs | P99 μs | queries |
|---|---:|---:|---:|---:|---:|
| lucene-10.3.0 | 1,437 | 502 | 2,421 | 11,013 | 943 |
| tantivy-0.25 | 1,784 | 430 | 2,737 | 18,933 | 943 |
| lance-f03a2783c24f-mt | 168,645 | 3,610 | 167,987 | 3,250,503 | 943 |
| lance-f03a2783c24f | 173,350 | 3,463 | 177,552 | 3,346,103 | 943 |

Do not read AVERAGE as "Lance retrieval is 120× slower". `the` alone is
**3.35s** on A1 vs **49μs** on Lucene. P50 is **~7×** Lucene (3.5ms vs 0.5ms).

### TOP_10_COUNT

| Engine | AVERAGE μs | P50 μs | P90 μs | P99 μs | queries |
|---|---:|---:|---:|---:|---:|
| tantivy-0.25 | 3,558 | 944 | 5,864 | 46,711 | 943 |
| lucene-10.3.0 | 4,917 | 1,195 | 8,325 | 63,805 | 943 |
| lance-f03a2783c24f-mt | 162,106 | 3,614 | 163,715 | 3,106,298 | 943 |
| lance-f03a2783c24f | 170,959 | 3,468 | 168,288 | 3,268,354 | 943 |

Lance `TOP_*_COUNT` is the same path as `COUNT` (`limit=None`, score every
match). Lucene/Tantivy can return top-k and a count without scoring the full
hit list the same way.

### TOP_100_COUNT

| Engine | AVERAGE μs | P50 μs | P90 μs | P99 μs | queries |
|---|---:|---:|---:|---:|---:|
| tantivy-0.25 | 3,581 | 969 | 5,884 | 46,666 | 943 |
| lucene-10.3.0 | 4,883 | 1,159 | 8,320 | 62,862 | 943 |
| lance-f03a2783c24f-mt | 162,433 | 3,589 | 164,809 | 3,112,728 | 943 |
| lance-f03a2783c24f | 174,638 | 3,496 | 180,889 | 3,400,161 | 943 |

## Latency by query shape

AVERAGE of best-of-10, microseconds.

### TOP_10

| Shape | n | lucene-10.3.0 | tantivy-0.25 | lance-f03a2783c24f | lance-f03a2783c24f-mt |
|---|---:|---:|---:|---:|---:|
| term | 1 | 2,342 | 2,315 | 709 | 852 |
| union | 301 | 1,181 | 1,822 | 2,490 | 2,586 |
| intersection | 300 | 949 | 1,181 | 2,092 | 2,301 |
| phrase | 300 | 1,981 | 1,802 | 3,800 | 4,172 |
| intersection_union | 40 | 3,016 | 3,207 | 25,142 | 25,152 |
| two-phase | 1 | 43,961 | 231,866 | 55,076 | 54,882 |

### TOP_100

| Shape | n | lucene-10.3.0 | tantivy-0.25 | lance-f03a2783c24f | lance-f03a2783c24f-mt |
|---|---:|---:|---:|---:|---:|
| term | 1 | 5,744 | 2,695 | 2,527 | 3,041 |
| union | 301 | 1,989 | 2,824 | 3,928 | 4,413 |
| intersection | 300 | 1,203 | 1,180 | 2,502 | 2,675 |
| phrase | 300 | 2,586 | 1,804 | 4,487 | 4,616 |
| intersection_union | 40 | 3,223 | 3,244 | 27,104 | 27,168 |
| two-phase | 1 | 42,828 | 231,604 | 71,354 | 71,824 |

### COUNT

| Shape | n | lucene-10.3.0 | tantivy-0.25 | lance-f03a2783c24f | lance-f03a2783c24f-mt |
|---|---:|---:|---:|---:|---:|
| term | 1 | 49 | 36 | 3,346,103 | 3,234,908 |
| union | 301 | 951 | 2,159 | 515,094 | 500,410 |
| intersection | 300 | 672 | 881 | 4,464 | 4,589 |
| phrase | 300 | 2,755 | 1,769 | 5,178 | 5,322 |
| intersection_union | 40 | 62 | 144 | 52,341 | 52,686 |
| two-phase | 1 | 37,657 | 231,200 | 93,162 | 92,754 |

Intersection / phrase COUNT is about **2×** Lucene. Union and `the` are the
scored-full-match cliff. `intersection_union` (Boolean MUST+SHOULD) is the
main TOP-k outlier: ~25ms vs Lucene ~3ms.

## How to read the comparison

1. **Matching is aligned.** 943/943 COUNT values match across all four engines.
2. **TOP-k latency is the fair retrieval comparison.** Lance A1 is roughly
   **2×** Lucene P50 and **2.5×** AVERAGE on `TOP_10` / `TOP_100`.
3. **COUNT AVERAGE is not a retrieval-kernel number for Lance.** It includes
   scoring every hit. Lucene/Tantivy have count-only collectors. Use COUNT P50,
   or intersection/phrase COUNT, if you want a less pathological view.
4. **A1 vs A2.** On this sequential harness, one partition + current-thread is
   slightly faster than four partitions + multi-thread Tokio.
5. **Do not compare top-k document ids** to Lucene; BM25 k1/b differ.

## Why Lance is slower (this run)

Three different mechanisms, not one "Lance kernel is 2× slower" story.

### Class 1 — per-query floor (~2× on typical AND / OR / phrase)

Cheapest AND/phrase on Lance A1 are **360–420μs**. Same queries on Lucene are
**80–145μs**, Tantivy **50–80μs**. That ~250–350μs does not shrink with hit
count.

Each bench command builds a new `Dataset::scan()` → DataFusion plan → stream.
Lucene/Tantivy reuse a long-lived `IndexSearcher` / `Searcher` and only parse
+ score. Lance also sits on Dataset / object-store / Session index cache even
when the files are already on tmpfs.

Evidence that this is a floor, not posting-walk cost:

- 35-hit `+kristanna +loken`: Lance 361μs vs Lucene 82μs (**4.4×**).
- AND P50 ratio vs Lucene is **2.9×** on 10–100 hits and **1.85×** on
  1k–10k hits. Union is **3.5×** on 1k–10k hits and **2.0×** on ≥100k hits.
- Phrase with ≥10k hits is **at or under** Lucene. High-df `the` TOP_10 is
  **0.30×** Lucene.

So on the leaf shapes that already have MAXSCORE / block AND / two-phase
phrase, the remaining gap is mostly **fixed planning + runtime tax**, plus
Lucene 10's vectorized block-max MAXSCORE / AND (Lance's summer 2026 work
aligned to Lucene 8–9 `MaxScoreBulkScorer`, not 10.x SIMD). Index size is
not the cause: FTS-only A1 is only ~15% larger than Lucene.

### Class 2 — Boolean MUST+SHOULD (~9×, the real TOP-k outlier)

40 `intersection_union` queries (`+climate policy`, `electric +vehicles`).
TOP_10 P50: Lance **16.4ms** vs Lucene **2.1ms** vs Tantivy **2.3ms**
(**~9×**, p90 ratio 11.7×). This does not shrink with the Class 1 floor.

These are not rewritten to `MatchQuery` AND/OR. They stay `FtsQuery::Boolean`.
The planner *can* take `CompoundQueryExec` + `ReqOptScorer` (MUST drives,
SHOULD rides). The fallback `BooleanQueryExec` is much worse: it strips
`limit`, scores every child to completion, HashMap-merges row ids, then
takes top-k (`scanner.rs` still has `TODO: rewrite the query for better
performance`).

This run used `fast_search` and a complete index, so TOP_10 should be on
ReqOpt, not the HashMap fallback. COUNT for the same shape is **52ms** vs
TOP_10 **25ms**, which also says TOP_10 is not scoring the full SHOULD
posting. The leftover 9× is therefore **ReqOpt / optional high-df SHOULD
still too expensive versus Lucene `ReqOptSumScorer`**: optional terms like
`policy` / `vehicles` / `markets` have huge postings; Lucene skips most of
them once the MUST score is already competitive. Lance's compound path
exists (`#8448`, ~1.3× in the 10M microbench) but is not yet in the same
class as Lucene 10 on this Wikipedia mix.

Worst examples (TOP_10 A1 / Lucene): `electric +vehicles` 14×, `+climate
policy` 12×, `+global markets` 12×.

### Class 3 — COUNT / `TOP_*_COUNT` (not a retrieval-kernel number)

Lance has no count-only collector. `limit=None` becomes `usize::MAX`: score
every match, emit row ids, count rows. Lucene `searcher.count()` / Tantivy
`Count` skip BM25; a single term can be `docFreq()` when the segment has no
deletes.

This bench has no deletes (`forceMerge(1)`, one Tantivy segment, no Lance
deletion vector), so Lucene `the` is **49μs** vs Lance **3.35s**. Union COUNT
AVERAGE (~0.5s) is the same cliff. AND/phrase COUNT stay about **2×**, which
matches Class 1.

### Not the cause on this run

- Stem / stopwords / take-back-to-table / wrong AND. Counts match 943/943.
- A2's four partitions. A1 (one partition, current-thread) is slightly
  *faster* on this sequential harness.
- Missing delete mask. There are no deletes here. With deletes, Lucene still
  only ANDs `liveDocs`; it does not start scoring.
- BM25 1.2/0.75 vs Lucene 0.9/0.4. That changes ranking, not this latency
  gap.

Raw timings: `results.json` on this branch. Summary script:
`python3 scripts/summarize_results.py results.json`.
