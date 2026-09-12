ENGINE_NAME := lance-f03a2783c24f-mt
SRC_ENGINE := ../lance-f03a2783c24f
INDEX_ROOT ?= /dev/shm/sbg/indexes
CARGO_TARGET_DIR ?= /tmp/sbg-target
export CARGO_TARGET_DIR
# Product-default search: unset shard overrides so create_index uses Lance defaults.
unexport LANCE_FTS_NUM_SHARDS
unexport LANCE_FTS_PARTITION_SIZE
export LANCE_BENCH_RUNTIME ?= multi_thread

BIN := $(CARGO_TARGET_DIR)/release

clean:
	rm -f idx
	rm -rf $(INDEX_ROOT)/$(ENGINE_NAME)

compile:
	@$(MAKE) -C $(SRC_ENGINE) compile CARGO_TARGET_DIR=$(CARGO_TARGET_DIR)

index: idx

serve:
	@LANCE_BENCH_RUNTIME=$(LANCE_BENCH_RUNTIME) $(BIN)/do_query idx

idx: compile
	@echo "---- Indexing lance-f03a2783c24f-mt (A2 default partitions) ----"
	@mkdir -p $(INDEX_ROOT)/$(ENGINE_NAME)
	@rm -f idx
	@ln -sfn $(INDEX_ROOT)/$(ENGINE_NAME) idx
	@env -u LANCE_FTS_NUM_SHARDS -u LANCE_FTS_PARTITION_SIZE -u LANCE_FTS_SEARCH_CHUNK \
		$(BIN)/build_index idx < $(CORPUS)
