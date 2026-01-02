#!/usr/bin/env python3
import sys
import json
import lancedb
from pathlib import Path
from time import perf_counter

def main():
    if len(sys.argv) < 2:
        print("Usage: build_index.py <index_dir>", file=sys.stderr)
        sys.exit(1)
    
    index_dir = Path(sys.argv[1])
    
    # 连接到LanceDB数据库
    db = lancedb.connect(index_dir)
    
    # 准备数据
    documents = []
    i = 0
    
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        
        i += 1
        if i % 100000 == 0:
            print(f"Processed {i} documents")
        
        try:
            data = json.loads(line)
            doc = {
                "id": data["id"],
                "text": data["text"],
            }
            documents.append(doc)
        except json.JSONDecodeError:
            continue
    
    print(f"Total documents to index: {len(documents)}")
    
    # 创建表
    if "wiki_articles" in db.table_names():
        db.drop_table("wiki_articles")
    
    table = db.create_table(
        "wiki_articles",
        data=documents,
        mode="overwrite"
    )
    
    # 使用Tantivy创建FTS索引
    start_time = perf_counter()
    print("Creating Tantivy FTS index (use_tantivy=True)...")
    table.create_fts_index("text", use_tantivy=True, with_position=True)
    elapsed_time = perf_counter() - start_time
    
    print(f"Tantivy FTS index created in {elapsed_time:.4f} seconds")
    print(f"Index created with {len(documents)} documents")
    print("Indexing completed successfully")

if __name__ == "__main__":
    main()