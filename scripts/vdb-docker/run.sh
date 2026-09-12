#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
FOYER_ROOT="${FOYER_ROOT:-/data/arrow/code/lance-distributed-foyer}"
OUT_DIR="${OUT_DIR:-${FOYER_ROOT}/.agent/fts-bench-vdb-docker/workspace}"
INDEX_ROOT="${INDEX_ROOT:-/data/db_datas/fts_search_data/sbg-wikipedia}"
LANCE_BIN_DIR="${LANCE_BIN_DIR:-/tmp/sbg-target}"
IMAGE="${IMAGE:-sbg-vdb-runtime:21}"
MEMORY="${MEMORY:-3g}"
MODE="${MODE:-bench}"
# Do not inherit leftover COMMANDS from the tmpfs Makefile (that set includes COUNT).
VDB_COMMANDS="${VDB_COMMANDS:-TOP_10 TOP_100}"
VDB_ENGINES="${VDB_ENGINES:-tantivy-0.25 lucene-10.3.0 lance-f03a2783c24f-mt lance-f03a2783c24f}"

mkdir -p "${OUT_DIR}"

if [[ ! -d "${INDEX_ROOT}/indexes/lucene-10.3.0" ]]; then
    echo "index snapshot missing under ${INDEX_ROOT}/indexes" >&2
    exit 1
fi
if [[ ! -x "${LANCE_BIN_DIR}/release/do_query" ]]; then
    echo "lance binary missing: ${LANCE_BIN_DIR}/release/do_query" >&2
    exit 1
fi

docker build -t "${IMAGE}" "${SCRIPT_DIR}"

if [[ "${SKIP_DROP_CACHES:-0}" != "1" ]]; then
    echo "=== host drop_caches ==="
    sync
    echo 3 > /proc/sys/vm/drop_caches
    echo "drop_caches done"
else
    echo "SKIP_DROP_CACHES=1"
fi

LOG="${OUT_DIR}/bench.log"
if [[ "$MODE" == "smoke" ]]; then
    LOG="${OUT_DIR}/smoke.log"
fi

echo "=== docker run memory=${MEMORY} mode=${MODE} ==="
set +e
docker run --rm --name "sbg-vdb-${MODE}" \
    --memory="${MEMORY}" --memory-swap="${MEMORY}" \
    -e JAVA_TOOL_OPTIONS="${JAVA_TOOL_OPTIONS:--Xmx512m -XX:+UseParallelGC}" \
    -e COMMANDS="${VDB_COMMANDS}" \
    -e ENGINES="${VDB_ENGINES}" \
    -e MODE="${MODE}" \
    -e WARMUP_TIME="${WARMUP_TIME:-60}" \
    -e NUM_ITER="${NUM_ITER:-10}" \
    -e CARGO_TARGET_DIR=/tmp/sbg-target \
    -e LANCE_BENCH_INDEX_CACHE_BYTES="${LANCE_BENCH_INDEX_CACHE_BYTES:-1536M}" \
    -e LANCE_BENCH_SKIP_PREWARM="${LANCE_BENCH_SKIP_PREWARM:-1}" \
    -e LANCE_BENCH_METADATA_CACHE_BYTES="${LANCE_BENCH_METADATA_CACHE_BYTES:-32M}" \
    -v "${BENCH_ROOT}:/bench:ro" \
    -v "${INDEX_ROOT}:/sbg" \
    -v "${LANCE_BIN_DIR}:/tmp/sbg-target:ro" \
    -v "${OUT_DIR}:/out" \
    "${IMAGE}" 2>&1 | tee "${LOG}"
rc=${PIPESTATUS[0]}
set -e
echo "DOCKER_EXIT:${rc}" | tee -a "${LOG}"
exit "${rc}"
