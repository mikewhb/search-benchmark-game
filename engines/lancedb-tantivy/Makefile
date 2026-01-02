clean:
	rm -fr idx
	rm -fr __pycache__
	rm -f check.log

compile:
	@echo "LanceDB with Tantivy FTS engine requires Python 3.8+, lancedb, and tantivy"

index: idx

serve: 
	@python3 do_query.py idx

serve-check:
	@python3 do_check.py idx

idx: idx/.indexed

idx/.indexed:
	@echo "\n\n\n---- Indexing LanceDB with Tantivy FTS ----"
	@mkdir -p idx
	python3 build_index.py "idx" < $(CORPUS)
	@touch idx/.indexed

test-index:
	@echo "\n\n\n---- Test Indexing LanceDB with Tantivy FTS ----"
	@mkdir -p idx
	python3 build_index.py "idx" < ../test_corpus.json
	@touch idx/.indexed