clean:
	@echo "--- Cleaning lance-duckdb ---"
	@rm -fr idx

compile:
	@python3 -c "import pymysql"

index: idx

idx: compile
	@echo "--- Indexing lance-duckdb ---"
	@python3 build_index.py idx < $(CORPUS)

serve: compile
	@python3 do_query.py idx
