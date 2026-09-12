ENGINE_NAME := lance-f03a2783c24f
INDEX_ROOT ?= /dev/shm/sbg/indexes
CARGO_TARGET_DIR ?= /tmp/sbg-target
export CARGO_TARGET_DIR
export LANCE_FTS_NUM_SHARDS ?= 1
export LANCE_FTS_PARTITION_SIZE ?= 65536
export LANCE_FTS_SEARCH_CHUNK ?= 1
export LANCE_BENCH_RUNTIME ?= current_thread

BIN := $(CARGO_TARGET_DIR)/release

clean:
	rm -f idx
	rm -rf $(INDEX_ROOT)/$(ENGINE_NAME)

compile: $(BIN)/build_index $(BIN)/do_query

index: idx

serve: $(BIN)/do_query
	@$(BIN)/do_query idx

$(BIN)/%:
	@echo "--- Building lance-f03a2783c24f $(notdir $@) ---"
	@RUSTFLAGS='-C target-cpu=native' cargo build --release --bin $(notdir $@)

idx: $(BIN)/build_index
	@echo "---- Indexing lance-f03a2783c24f (A1 single partition) ----"
	@mkdir -p $(INDEX_ROOT)/$(ENGINE_NAME)
	@rm -f idx
	@ln -sfn $(INDEX_ROOT)/$(ENGINE_NAME) idx
	@LANCE_FTS_NUM_SHARDS=$(LANCE_FTS_NUM_SHARDS) \
		LANCE_FTS_PARTITION_SIZE=$(LANCE_FTS_PARTITION_SIZE) \
		$(BIN)/build_index idx < $(CORPUS)
