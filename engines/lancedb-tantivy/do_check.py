#!/usr/bin/env python3
"""
LanceDB Tantivy FTS 查询检查工具
用于记录实际执行的查询和查询结果到日志文件
"""
import warnings
import sys
import os
import re
import json
import lancedb
from pathlib import Path
from datetime import datetime

# 日志文件路径
LOG_FILE = Path(__file__).parent / "check.log"


def parse_query(query_str):
    """
    解析 Tantivy 风格的查询字符串，返回 (query_type, terms)
    """
    query_str = query_str.strip()
    
    if query_str.startswith('"') and query_str.endswith('"'):
        inner = query_str[1:-1]
        return ("phrase", inner)
    
    if query_str.startswith('+'):
        terms = re.findall(r'\+(\S+)', query_str)
        if terms:
            reconstructed = ' '.join(['+' + t for t in terms])
            if reconstructed == query_str:
                return ("intersection", ' '.join(terms))
    
    words = query_str.split()
    if len(words) == 1:
        return ("term", query_str)
    
    return ("union", query_str)


def execute_fts_query(table, query_str, limit):
    """根据查询类型执行 FTS 查询"""
    query_type, terms = parse_query(query_str)
    
    if query_type == "phrase":
        return table.search(terms, query_type="fts").phrase_query(True).select(["id"]).limit(limit)
    elif query_type == "intersection":
        return table.search(terms, query_type="fts").phrase_query(False).select(["id"]).limit(limit)
    elif query_type == "union":
        return table.search(terms, query_type="fts").phrase_query(False).select(["id"]).limit(limit)
    else:
        return table.search(terms, query_type="fts").select(["id"]).limit(limit)


def log_query(log_file, original_query, query_type, actual_query, command, result_count, result_ids):
    """记录查询到日志文件"""
    log_entry = {
        "timestamp": datetime.now().isoformat(),
        "original_query": original_query,
        "query_type": query_type,
        "actual_query": actual_query,
        "command": command,
        "result_count": result_count,
        "result_ids": result_ids[:20]
    }
    log_file.write(json.dumps(log_entry, ensure_ascii=False) + "\n")
    log_file.flush()


def main():
    if len(sys.argv) < 2:
        print("Usage: do_check.py <index_dir>", file=sys.stderr)
        sys.exit(1)
    
    index_dir = Path(sys.argv[1])
    
    db = lancedb.connect(index_dir)
    table = db.open_table("wiki_articles")
    
    with open(LOG_FILE, "w", encoding="utf-8") as log_file:
        log_file.write(f"# LanceDB Tantivy FTS Check Log - {datetime.now().isoformat()}\n")
        log_file.write(f"# Index: {index_dir}\n\n")
        
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            
            fields = line.split("\t")
            if len(fields) != 2:
                print("UNSUPPORTED", flush=True)
                continue
            
            command = fields[0]
            query_str = fields[1]
            
            try:
                query_type, actual_terms = parse_query(query_str)
                
                if query_type == "phrase":
                    actual_query = f'phrase_query(True): "{actual_terms}"'
                elif query_type == "intersection":
                    actual_query = f'phrase_query(False) [OR]: "{actual_terms}"'
                elif query_type == "union":
                    actual_query = f'phrase_query(False) [OR]: "{actual_terms}"'
                else:
                    actual_query = f'term: "{actual_terms}"'
                
                if command in ["COUNT", "TOP_10", "TOP_100", "TOP_1000"]:
                    limit = {"COUNT": 1000000, "TOP_10": 10, "TOP_100": 100, "TOP_1000": 1000}[command]
                    result = execute_fts_query(table, query_str, limit).to_arrow()
                    result_count = result.num_rows
                    
                    result_ids = []
                    if result.num_rows > 0:
                        id_column = result.column("id")
                        result_ids = [str(id_column[i].as_py()) for i in range(min(20, result.num_rows))]
                    
                    log_query(log_file, query_str, query_type, actual_query, command, result_count, result_ids)
                    print(result_count if command == "COUNT" else (result_count if result_count > 0 else 1), flush=True)
                
                elif command in ["TOP_1_COUNT", "TOP_5_COUNT", "TOP_10_COUNT", "TOP_100_COUNT", "TOP_1000_COUNT"]:
                    limit_map = {"TOP_1_COUNT": 1, "TOP_5_COUNT": 5, "TOP_10_COUNT": 10, "TOP_100_COUNT": 100, "TOP_1000_COUNT": 1000}
                    limit = limit_map[command]
                    
                    result = execute_fts_query(table, query_str, limit).to_arrow()
                    count_result = execute_fts_query(table, query_str, 100000).to_arrow()
                    
                    result_ids = []
                    if result.num_rows > 0:
                        id_column = result.column("id")
                        result_ids = [str(id_column[i].as_py()) for i in range(min(20, result.num_rows))]
                    
                    log_query(log_file, query_str, query_type, actual_query, command, count_result.num_rows, result_ids)
                    print(count_result.num_rows, flush=True)
                
                else:
                    print("UNSUPPORTED", flush=True)
            
            except Exception as e:
                log_file.write(f"ERROR: {query_str} -> {e}\n")
                log_file.flush()
                print("UNSUPPORTED", flush=True)

if __name__ == "__main__":
    main()
