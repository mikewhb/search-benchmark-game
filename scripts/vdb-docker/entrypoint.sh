#!/bin/bash
set -euo pipefail

ENGINES="${ENGINES:-tantivy-0.25 lucene-10.3.0 lance-f03a2783c24f-mt lance-f03a2783c24f}"
COMMANDS="${COMMANDS:-TOP_10 TOP_100}"
MODE="${MODE:-bench}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/sbg-target}"
export COMMANDS

if [[ ! -d /bench/src || ! -d /sbg/indexes ]]; then
    echo "missing mounts: /bench and /sbg" >&2
    exit 1
fi

rm -rf /work/sbg
mkdir -p /work/sbg/src /work/sbg/engines
cp /bench/src/client.py /work/sbg/src/client.py
cp /bench/queries.txt /work/sbg/queries.txt

for engine in $ENGINES; do
    src="/bench/engines/${engine}"
    dst="/work/sbg/engines/${engine}"
    if [[ ! -d "$src" ]]; then
        echo "engine dir missing: $src" >&2
        exit 1
    fi
    mkdir -p "$dst"
    cp "$src/Makefile" "$dst/Makefile"
    if [[ -f "$src/details.json" ]]; then
        cp "$src/details.json" "$dst/details.json"
    fi
    if [[ ! -d "/sbg/indexes/${engine}" ]]; then
        echo "index missing: /sbg/indexes/${engine}" >&2
        exit 1
    fi
    ln -sfn "/sbg/indexes/${engine}" "${dst}/idx"

    case "$engine" in
        lucene-*)
            cp -a "$src/build" "${dst}/build"
            ;;
        tantivy-*)
            mkdir -p "${dst}/target/release"
            ln -sfn "${src}/target/release/do_query" "${dst}/target/release/do_query"
            ln -sfn "${src}/target/release/build_index" "${dst}/target/release/build_index" 2>/dev/null || true
            ;;
    esac
done

if [[ ! -x "${CARGO_TARGET_DIR}/release/do_query" ]]; then
    echo "lance do_query missing: ${CARGO_TARGET_DIR}/release/do_query" >&2
    exit 1
fi

echo "=== vdb-docker runtime ==="
echo "MODE=${MODE}"
echo "COMMANDS=${COMMANDS}"
echo "ENGINES=${ENGINES}"
echo "WARMUP_TIME=${WARMUP_TIME:-60} NUM_ITER=${NUM_ITER:-10}"
echo "JAVA_TOOL_OPTIONS=${JAVA_TOOL_OPTIONS:-}"
echo "CARGO_TARGET_DIR=${CARGO_TARGET_DIR}"
free -h || true
echo "--- idx ---"
for engine in $ENGINES; do
    echo -n "${engine}: "
    readlink -f "/work/sbg/engines/${engine}/idx"
done

smoke_one() {
    local engine="$1"
    local cwd="/work/sbg/engines/${engine}"
    echo "--- smoke ${engine} ---"
    # One query then close stdin so serve exits.
    printf 'TOP_10\tthe\n' | make --no-print-directory -C "$cwd" serve
}

if [[ "$MODE" == "smoke" ]]; then
    for engine in $ENGINES; do
        smoke_one "$engine"
    done
    echo "SMOKE_OK"
    exit 0
fi

cd /work/sbg
# client.py writes results.json to CWD.
python3 -u src/client.py queries.txt $ENGINES
if [[ -d /out ]]; then
    cp -f /work/sbg/results.json /out/results.json
    echo "wrote /out/results.json"
fi
echo "BENCH_EXIT:0"
