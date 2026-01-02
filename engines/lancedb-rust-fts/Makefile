INDEX_DIR ?= ../../indexes/lancedb-rust-fts
ENGINE_DIR = $(shell pwd)

.PHONY: all compile index query serve serve-check clean

all: compile

clean:
	rm -fr idx
	rm -fr target
	rm -f check.log

compile: target/release/build_index target/release/do_query target/release/do_check

index: idx compile

serve: target/release/do_query 
	@target/release/do_query idx

serve-check: target/release/do_check
	@target/release/do_check idx

target/release/%:
	@echo "\n\n\n--- Building lancedb-rust-fts binary ---" >&2
	@RUSTFLAGS='-C target-cpu=native' cargo build --release --bin $(notdir $@) >&2

idx: target/release/build_index
	@echo "\n\n\n---- Indexing lancedb-rust-fts ----"
	@mkdir -p idx
	export RUST_LOG=info && target/release/build_index "$@" < ${CORPUS}