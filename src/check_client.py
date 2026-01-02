"""
Check 客户端 - 用于发送查询到 do_check 应用并记录日志
"""
import subprocess
import sys
import os
from os import path
import json

COMMANDS = os.environ.get('COMMANDS', 'TOP_10 TOP_100 TOP_1000').split(' ')
DEBUG = os.environ.get('DEBUG', '0') == '1'

class CheckClient:
    def __init__(self, engine):
        self.engine = engine
        dirname = os.path.split(os.path.abspath(__file__))[0]
        dirname = path.dirname(dirname)
        dirname = path.join(dirname, "engines")
        cwd = path.join(dirname, engine)
        print(f"Starting check for {engine} in {cwd}")
        self.process = subprocess.Popen(
            ["make", "--no-print-directory", "serve-check"],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stdin=subprocess.PIPE,
            stderr=subprocess.PIPE
        )

    def query(self, query, command):
        query_line = "%s\t%s\n" % (command, query)
        if DEBUG:
            print(f"DEBUG CHECK: Sending query: {command}\t{query[:50]}...", flush=True)
        self.process.stdin.write(query_line.encode("utf-8"))
        self.process.stdin.flush()
        recv = self.process.stdout.readline().strip()
        if DEBUG:
            print(f"DEBUG CHECK: Received: {recv}", flush=True)
        if recv == b"UNSUPPORTED":
            return None
        if recv == b"":
            return None
        try:
            cnt = int(recv)
            return cnt
        except ValueError:
            return None

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        # 等待进程结束
        self.process.wait()

class Query:
    def __init__(self, query, tags):
        self.query = query
        self.tags = tags

def read_queries(query_path):
    for q in open(query_path):
        c = json.loads(q)
        yield Query(c["query"], c["tags"])

if __name__ == "__main__":
    random_seed = 2
    query_path = sys.argv[1]
    engines = sys.argv[2:]
    queries = list(read_queries(query_path))
    
    print(f"Loaded {len(queries)} queries from {query_path}")
    print(f"Engines to check: {engines}")
    print(f"Commands: {COMMANDS}")
    
    for engine in engines:
        for command in COMMANDS:
            print(f"\n======================")
            print(f"CHECKING {engine} {command}")
            check_client = CheckClient(engine)
            
            success_count = 0
            fail_count = 0
            
            for i, query in enumerate(queries):
                result = check_client.query(query.query, command)
                if result is not None:
                    success_count += 1
                else:
                    fail_count += 1
                
                # 显示进度
                if (i + 1) % 100 == 0 or (i + 1) == len(queries):
                    print(f"Progress: {i+1}/{len(queries)} (success={success_count}, fail={fail_count})", flush=True)
            
            check_client.close()
            print(f"Completed: {engine} {command} - success={success_count}, fail={fail_count}")
            print(f"Log file: engines/{engine}/check.log")
    
    print("\n======================")
    print("Check completed!")
    print("Log files are in each engine's directory (check.log)")
