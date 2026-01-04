clean:
	rm -fr idx
	rm -fr target
	rm -f check.log

compile: target/release/build_index target/release/do_query target/release/do_check

index: idx

serve: target/release/do_query
	@target/release/do_query idx

serve-check: target/release/do_check
	@target/release/do_check idx

target/release/%:
	@echo "\n\n\n--- Building tantivy's binary ---"
	@RUSTFLAGS='-C target-cpu=native' cargo build --release --bin $(notdir $@)

idx: target/release/build_index
	@echo "\n\n\n---- Indexing tantivy ----"
	@mkdir -p idx
	export RUST_LOG=info && target/release/build_index "$@" < ${CORPUS}
