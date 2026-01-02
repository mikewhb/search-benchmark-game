COMMANDS ?= TOP_10 TOP_100 TOP_1000
ENGINES ?= lancedb-rust-fts tantivy-0.22 lancedb-lancefts lancedb-tantivy

tantivy-0.22 used original code, the only change is to store the text filed in the .store file by {"text", TEXT | STORED}

python env:
pip install -r engines/lancedb-lancefts/requirements.txt
pip install -r engines/lancedb-tantivy/requirements.txt

DataSize:
original json file:
~/Code/search-benchmark-game 02:59:35]$ls -lrh *json
-rw-r--r--@ 1 mike  admin   7.6G Dec 31 17:30 corpus.json

lancedb's size:
~/Code/search-benchmark-game/engines/lancedb-rust-fts/idx/wiki_articles.lance 03:09:31]$du -hd 1
240K    ./_versions
208K    ./_transactions
7.6G    ./data
17G    ./_indices
25G    .

tantivy's size:

~/Code/search-benchmark-game/engines/tantivy-0.22/idx 03:12:04
du -hd 1
7.3G    .

~/Code/search-benchmark-game/engines/tantivy-0.22/idx 03:12:07
ls -lrth
total 15263600
-rw-r--r--@ 1 mike  admin   4.8M Jan  2 12:55 a996bdeb51aa4df697b7b88aae766ab7.fieldnorm
-rw-r--r--@ 1 mike  admin   146B Jan  2 12:56 a996bdeb51aa4df697b7b88aae766ab7.fast
-rw-r--r--@ 1 mike  admin    45M Jan  2 12:56 a996bdeb51aa4df697b7b88aae766ab7.term
-rw-r--r--@ 1 mike  admin   1.0G Jan  2 12:56 a996bdeb51aa4df697b7b88aae766ab7.idx
-rw-r--r--@ 1 mike  admin   1.7G Jan  2 12:56 a996bdeb51aa4df697b7b88aae766ab7.pos
-rw-r--r--@ 1 mike  admin   4.5G Jan  2 12:56 a996bdeb51aa4df697b7b88aae766ab7.store
-rw-------@ 1 mike  admin   664B Jan  2 12:56 meta.json

perfmance:


TOP_10:

| Query | lancedb-rust-fts | tantivy-0.22 | lancedb-lancefts | lancedb-tantivy |
|-------|------------------|--------------|------------------|-----------------|
| AVERAGE | 7,121 μs | 1,251 μs | 7,664 μs | 11,160 μs |
| P50 | 5,850 μs | 419 μs | 6,262 μs | 7,720 μs |
| P90 | 10,506 μs | 2,073 μs | 11,413 μs | 16,985 μs |
| P99 | 22,325 μs | 12,274 μs | 24,212 μs | 49,094 μs |

TOP_100:

| Query | lancedb-rust-fts | tantivy-0.22 | lancedb-lancefts | lancedb-tantivy |
|-------|------------------|--------------|------------------|-----------------|
| AVERAGE | 9,197 μs | 1,429 μs | 10,022 μs | 14,441 μs |
| P50 | 7,791 μs | 466 μs | 8,478 μs | 11,245 μs |
| P90 | 14,188 μs | 2,532 μs | 15,513 μs | 19,911 μs |
| P99 | 28,682 μs | 13,692 μs | 31,768 μs | 53,020 μs |

TOP_1000:

| Query | lancedb-rust-fts | tantivy-0.22 | lancedb-lancefts | lancedb-tantivy |
|-------|------------------|--------------|------------------|-----------------|
| AVERAGE | 15,905 μs | 1,846 μs | 19,161 μs | 33,108 μs |
| P50 | 14,665 μs | 651 μs | 19,284 μs | 35,498 μs |
| P90 | 27,651 μs | 4,091 μs | 31,836 μs | 46,610 μs |
| P99 | 46,057 μs | 17,508 μs | 50,463 μs | 82,909 μs |