#!/usr/bin/env python3
import sys
import os
import json
import lancedb
from pathlib import Path
from time import perf_counter

# 设置 Lance FTS 并行参数（环境变量）
# LANCE_FTS_NUM_SHARDS: 工作线程数，默认为 CPU 核心数
# LANCE_FTS_FLUSH_SIZE: 刷写阈值（字节），默认 16MB，增大可减少刷写次数
# LANCE_FTS_PARTITION_SIZE: 分区大小限制（字节），默认 256MB
num_cpus = os.cpu_count() or 8
os.environ.setdefault("LANCE_FTS_NUM_SHARDS", str(num_cpus))
os.environ.setdefault("LANCE_FTS_FLUSH_SIZE", str(64 * 1024 * 1024))  # 64MB，减少刷写次数
os.environ.setdefault("LANCE_FTS_PARTITION_SIZE", str(512 * 1024 * 1024))  # 512MB，更大分区

BATCH_SIZE = 500000  # 每批处理50万条

def main():
    if len(sys.argv) < 2:
        print("Usage: build_index.py <index_dir>", file=sys.stderr)
        sys.exit(1)
    
    index_dir = Path(sys.argv[1])
    
    print(f"LANCE_FTS_NUM_SHARDS = {os.environ.get('LANCE_FTS_NUM_SHARDS')}")
    print(f"LANCE_FTS_FLUSH_SIZE = {os.environ.get('LANCE_FTS_FLUSH_SIZE')} bytes")
    print(f"LANCE_FTS_PARTITION_SIZE = {os.environ.get('LANCE_FTS_PARTITION_SIZE')} bytes")
    
    # 连接到LanceDB数据库
    db = lancedb.connect(index_dir)
    
    # 删除旧表
    if "wiki_articles" in db.list_tables():
        db.drop_table("wiki_articles")
    
    # 流式读取和批量写入
    documents = []
    i = 0
    table = None
    total_docs = 0
    
    data_start = perf_counter()
    
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        
        i += 1
        
        try:
            data = json.loads(line)
            documents.append({
                "id": data["id"],
                "text": data["text"],
            })
        except json.JSONDecodeError:
            continue
        
        # 批量写入
        if len(documents) >= BATCH_SIZE:
            if table is None:
                table = db.create_table("wiki_articles", data=documents, mode="overwrite")
            else:
                table.add(documents)
            total_docs += len(documents)
            print(f"Written {total_docs} documents...")
            documents = []
    
    # 写入剩余数据
    if documents:
        if table is None:
            table = db.create_table("wiki_articles", data=documents, mode="overwrite")
        else:
            table.add(documents)
        total_docs += len(documents)
    
    data_elapsed = perf_counter() - data_start
    print(f"Data loading completed: {total_docs} documents in {data_elapsed:.2f}s")
    
    # 创建FTS索引 - 使用Lance原生FTS（底层Rust并行）
    start_time = perf_counter()
    print("Creating Lance native FTS index (use_tantivy=False, with_position=True)...")
    table.create_fts_index("text", use_tantivy=False, with_position=True)
    elapsed_time = perf_counter() - start_time
    
    print(f"FTS index created in {elapsed_time:.2f} seconds")
    print(f"Total: {total_docs} documents indexed")
    print("Indexing completed successfully")

if __name__ == "__main__":
    main()