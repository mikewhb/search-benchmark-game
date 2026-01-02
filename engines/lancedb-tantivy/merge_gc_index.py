#!/usr/bin/env python3
"""
对 LanceDB Tantivy FTS 索引进行 merge 和 gc 操作

使用方法：
    python3 merge_gc_index.py <index_dir>
    
示例：
    python3 merge_gc_index.py idx/wiki_articles.lance
"""

import os
import sys
from time import perf_counter

import lancedb


def get_index_info(table):
    """获取索引信息"""
    try:
        indices = table.list_indices()
        print(f"当前索引列表: {indices}")
        return indices
    except Exception as e:
        print(f"获取索引信息失败: {e}")
        return []


def optimize_fts_index(db_path: str, table_name: str = "documents"):
    """
    优化 FTS 索引，执行 merge 和 gc 操作
    
    Args:
        db_path: LanceDB 数据库路径
        table_name: 表名
    """
    print(f"连接数据库: {db_path}")
    db = lancedb.connect(db_path)
    
    # 列出所有表
    tables = db.table_names()
    print(f"数据库中的表: {tables}")
    
    if table_name not in tables:
        # 尝试找到第一个可用的表
        if tables:
            table_name = tables[0]
            print(f"使用表: {table_name}")
        else:
            print("错误: 数据库中没有表")
            return
    
    table = db.open_table(table_name)
    print(f"打开表: {table_name}, 行数: {table.count_rows()}")
    
    # 获取索引信息
    get_index_info(table)
    
    # 获取 Lance 数据集进行底层操作
    dataset = table.to_lance()
    
    print("\n" + "=" * 50)
    print("开始优化 Lance 数据集...")
    print("=" * 50)
    
    # 1. 压缩数据文件 (Compaction)
    print("\n[1/3] 执行数据压缩 (Compaction)...")
    start_time = perf_counter()
    try:
        # 压缩小文件，合并成更大的文件
        stats = dataset.optimize.compact_files(
            target_rows_per_fragment=1024 * 1024,  # 每个 fragment 目标行数
            max_rows_per_group=1024,  # 每个 row group 最大行数
            num_threads=os.cpu_count() or 4
        )
        elapsed = perf_counter() - start_time
        print(f"   压缩完成，耗时: {elapsed:.2f}s")
        print(f"   压缩统计: {stats}")
    except Exception as e:
        print(f"   压缩失败: {e}")
    
    # 2. 清理旧版本 (Cleanup old versions)
    print("\n[2/3] 清理旧版本...")
    start_time = perf_counter()
    try:
        # 清理超过一定时间的旧版本
        stats = dataset.cleanup_old_versions(
            older_than=None,  # 清理所有旧版本，保留最新
            delete_unverified=True
        )
        elapsed = perf_counter() - start_time
        print(f"   清理完成，耗时: {elapsed:.2f}s")
        print(f"   清理统计: {stats}")
    except Exception as e:
        print(f"   清理失败: {e}")
    
    # 3. 优化索引
    print("\n[3/3] 优化索引...")
    start_time = perf_counter()
    try:
        stats = dataset.optimize.optimize_indices(num_indices_to_merge=None)
        elapsed = perf_counter() - start_time
        print(f"   索引优化完成，耗时: {elapsed:.2f}s")
        print(f"   优化统计: {stats}")
    except Exception as e:
        print(f"   索引优化失败: {e}")
    
    print("\n" + "=" * 50)
    print("优化完成!")
    print("=" * 50)


def main():
    if len(sys.argv) < 2:
        # 默认使用当前目录下的索引
        script_dir = os.path.dirname(os.path.abspath(__file__))
        db_path = os.path.join(script_dir, "idx", "wiki_articles.lance")
    else:
        db_path = sys.argv[1]
    
    if not os.path.exists(db_path):
        print(f"错误: 路径不存在: {db_path}")
        sys.exit(1)
    
    # 检查是否是 .lance 目录
    if not db_path.endswith(".lance"):
        # 尝试查找 .lance 目录
        lance_path = db_path + ".lance"
        if os.path.exists(lance_path):
            db_path = lance_path
        else:
            # 可能是 LanceDB 数据库目录
            pass
    
    # 如果是 .lance 文件，需要找到父目录作为数据库路径
    if db_path.endswith(".lance"):
        # LanceDB 的数据库路径是 .lance 文件的父目录
        parent_dir = os.path.dirname(db_path)
        table_name = os.path.basename(db_path).replace(".lance", "")
        optimize_fts_index(parent_dir, table_name)
    else:
        optimize_fts_index(db_path)


if __name__ == "__main__":
    main()
