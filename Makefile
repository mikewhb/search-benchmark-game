SBG_ROOT ?= /dev/shm/sbg
CORPUS ?= $(SBG_ROOT)/corpus.json
JAVA_HOME ?= /tmp/jdk-21.0.8+9
PATH := $(JAVA_HOME)/bin:$(PATH)
export

WIKI_SRC = "https://www.dropbox.com/s/wwnfnu441w1ec9p/wiki-articles.json.bz2?dl=1"

COMMANDS ?= TOP_10 TOP_100 COUNT TOP_10_COUNT TOP_100_COUNT
ENGINES ?= tantivy-0.25 lucene-10.3.0 lance-f03a2783c24f-mt lance-f03a2783c24f
PORT ?= 8080
WARMUP_TIME ?= 60

help:
	@grep '^[^#[:space:]].*:' Makefile

all: index

corpus:
	@echo "--- Downloading $(WIKI_SRC) to $(CORPUS) ---"
	@mkdir -p $(dir $(CORPUS))
	@curl -# -L "$(WIKI_SRC)" | bunzip2 -c | python3 corpus_transform.py > $(CORPUS)

prepare-idx:
	@mkdir -p $(SBG_ROOT)/indexes
	@for engine in $(ENGINES); do \
		mkdir -p $(SBG_ROOT)/indexes/$$engine; \
		rm -rf ${shell pwd}/engines/$$engine/idx; \
		ln -sfn $(SBG_ROOT)/indexes/$$engine ${shell pwd}/engines/$$engine/idx; \
	done

clean:
	@echo "--- Cleaning directories ---"
	@rm -fr results
	@for engine in $(ENGINES); do cd ${shell pwd}/engines/$$engine && make clean ; done

index: prepare-idx
	@echo "--- Indexing corpus ---"
	@for engine in $(ENGINES); do cd ${shell pwd}/engines/$$engine && make index ; done

bench:
	@echo "--- Benchmarking ---"
	@rm -fr results
	@mkdir results
	@python3 src/client.py queries.txt $(ENGINES)

compile:
	@echo "--- Compiling binaries ---"
	@for engine in $(ENGINES); do cd ${shell pwd}/engines/$$engine && make compile ; done

serve:
	@echo "--- Serving results ---"
	@cp results.json web/build/results.json
	@cd web/build && python3 -m http.server $(PORT)
