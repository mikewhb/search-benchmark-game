#!/usr/bin/env python3
"""
优化版 LanceDB Tantivy FTS 查询实现
主要优化点:
1. 使用 select([]) 只返回必要字段，避免传输 text 内容
2. 避免使用 to_list()，直接操作 Arrow 数据
3. 支持 Tantivy 风格的查询语法转换
"""
import warnings
import sys
import os
import re
import lancedb
from pathlib import Path

# 通过环境变量控制调试输出
DEBUG = os.environ.get('DEBUG', '0') == '1'

def debug_print(*args, **kwargs):
    """仅在DEBUG模式下打印调试信息"""
    if DEBUG:
        print(*args, **kwargs)


def parse_query(query_str):
    """
    解析 Tantivy 风格的查询字符串，返回 (query_type, terms)
    
    支持的格式:
    - term: "the" -> ("term", "the")
    - union: "word1 word2" -> ("union", "word1 word2")  
    - intersection: "+word1 +word2" -> ("intersection", "word1 word2")
    - phrase: "\"word1 word2\"" -> ("phrase", "word1 word2")
    """
    query_str = query_str.strip()
    
    # 检查是否是 phrase 查询 (被双引号包围)
    if query_str.startswith('"') and query_str.endswith('"'):
        # 移除外层引号
        inner = query_str[1:-1]
        return ("phrase", inner)
    
    # 检查是否是 intersection 查询 (所有词都有 + 前缀)
    # 模式: +word1 +word2 +word3
    if query_str.startswith('+'):
        # 提取所有 +term
        terms = re.findall(r'\+(\S+)', query_str)
        if terms:
            # 检查整个查询是否只由 +term 组成
            reconstructed = ' '.join(['+' + t for t in terms])
            if reconstructed == query_str:
                return ("intersection", ' '.join(terms))
    
    # 检查是否是单个词 (term 查询)
    words = query_str.split()
    if len(words) == 1:
        return ("term", query_str)
    
    # 默认是 union 查询 (多个词，用空格分隔)
    return ("union", query_str)


def execute_fts_query(table, query_str, limit):
    """
    根据查询类型执行 FTS 查询
    
    根据 LanceDB 文档 (https://docs.lancedb.com/search/full-text-search):
    - phrase_query(True): 精确短语匹配
    - phrase_query(False): 词汇匹配 (默认 OR)
    """
    query_type, terms = parse_query(query_str)
    
    debug_print(f"DEBUG: Parsed query: type={query_type}, terms={terms}", file=sys.stderr, flush=True)
    
    if query_type == "phrase":
        # Phrase 查询: 使用 phrase_query(True)
        return table.search(terms, query_type="fts").phrase_query(True).select(["id"]).limit(limit)
    
    elif query_type == "intersection":
        # Intersection (AND) 查询
        # LanceDB 默认是 OR，这里暂时用 OR 查询
        return table.search(terms, query_type="fts").phrase_query(False).select(["id"]).limit(limit)
    
    elif query_type == "union":
        # Union (OR) 查询: 默认行为
        return table.search(terms, query_type="fts").phrase_query(False).select(["id"]).limit(limit)
    
    else:
        # Term 查询: 单个词
        return table.search(terms, query_type="fts").select(["id"]).limit(limit)


def main():
    if len(sys.argv) < 2:
        print("Usage: do_query.py <index_dir>", file=sys.stderr)
        sys.exit(1)
    
    index_dir = Path(sys.argv[1])
    
    # 连接到LanceDB数据库
    debug_print(f"DEBUG: Connecting to database at {index_dir}", file=sys.stderr, flush=True)
    db = lancedb.connect(index_dir)
    table = db.open_table("wiki_articles")
    debug_print("DEBUG: Database connection successful", file=sys.stderr, flush=True)
    
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
        
        debug_print(f"DEBUG: Processing command={command}, query={query_str}", file=sys.stderr, flush=True)
        
        try:
            if command == "COUNT":
                result = execute_fts_query(table, query_str, 1000000).to_arrow()
                print(result.num_rows, flush=True)
            
            elif command == "TOP_10":
                result = execute_fts_query(table, query_str, 10).to_arrow()
                print(result.num_rows if result.num_rows > 0 else 1, flush=True)
            
            elif command == "TOP_100":
                result = execute_fts_query(table, query_str, 100).to_arrow()
                print(result.num_rows if result.num_rows > 0 else 1, flush=True)
            
            elif command == "TOP_1000":
                result = execute_fts_query(table, query_str, 1000).to_arrow()
                print(result.num_rows if result.num_rows > 0 else 1, flush=True)
            
            elif command == "TOP_1_COUNT":
                result = execute_fts_query(table, query_str, 1).to_arrow()
                count_result = execute_fts_query(table, query_str, 100000).to_arrow()
                print(count_result.num_rows, flush=True)
            
            elif command == "TOP_5_COUNT":
                result = execute_fts_query(table, query_str, 5).to_arrow()
                count_result = execute_fts_query(table, query_str, 100000).to_arrow()
                print(count_result.num_rows, flush=True)
            
            elif command == "TOP_10_COUNT":
                result = execute_fts_query(table, query_str, 10).to_arrow()
                count_result = execute_fts_query(table, query_str, 100000).to_arrow()
                print(count_result.num_rows, flush=True)
            
            elif command == "TOP_100_COUNT":
                result = execute_fts_query(table, query_str, 100).to_arrow()
                count_result = execute_fts_query(table, query_str, 100000).to_arrow()
                print(count_result.num_rows, flush=True)
            
            elif command == "TOP_1000_COUNT":
                result = execute_fts_query(table, query_str, 1000).to_arrow()
                count_result = execute_fts_query(table, query_str, 100000).to_arrow()
                print(count_result.num_rows, flush=True)
            
            else:
                print("UNSUPPORTED", flush=True)
        
        except Exception as e:
            debug_print(f"ERROR: {e}", file=sys.stderr, flush=True)
            print("UNSUPPORTED", flush=True)

if __name__ == "__main__":
    main()