import subprocess
import sys
import os
from os import path
import time
import json
import random
from collections import defaultdict

COMMANDS = os.environ['COMMANDS'].split(' ')
DEBUG = os.environ.get('DEBUG', '0') == '1'

class SearchClient:

    def __init__(self, engine):
        self.engine = engine
        dirname = os.path.split(os.path.abspath(__file__))[0]
        dirname = path.dirname(dirname)
        dirname = path.join(dirname, "engines")
        cwd = path.join(dirname, engine)
        print(cwd)
        self.process = subprocess.Popen(["make", "--no-print-directory", "serve"],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stdin=subprocess.PIPE)

    def query(self, query, command):
        query_line = "%s\t%s\n" % (command, query)
        if DEBUG:
            print(f"\nDEBUG CLIENT: Sending query: {command}\t{query[:50]}...", flush=True)
        self.process.stdin.write(query_line.encode("utf-8"))
        self.process.stdin.flush()
        if DEBUG:
            print(f"DEBUG CLIENT: Waiting for response...", flush=True)
        recv = self.process.stdout.readline().strip()
        if DEBUG:
            print(f"DEBUG CLIENT: Received: {recv}", flush=True)
        if recv == b"UNSUPPORTED":
            return None
        if recv == b"":
            # 空响应，可能是子进程崩溃或管道断开
            print(f"ERROR: Empty response received for query: {query[:50]}...", file=sys.stderr, flush=True)
            print(f"ERROR: Engine process may have crashed. Check stderr for details.", file=sys.stderr, flush=True)
            # 检查子进程状态
            ret = self.process.poll()
            if ret is not None:
                print(f"ERROR: Engine process exited with code: {ret}", file=sys.stderr, flush=True)
            return None
        try:
            cnt = int(recv)
            return cnt
        except ValueError:
            print(f"ERROR: Invalid response '{recv}' for query: {query[:50]}...", file=sys.stderr, flush=True)
            return None

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()

def drive(queries, client, command):
    for i, query in enumerate(queries):
        start = time.monotonic()
        count = client.query(query.query, command)
        stop = time.monotonic()
        duration = int((stop - start) * 1e6)
        if count is None:
            if DEBUG:
                print(f"DEBUG CLIENT: Query {i+1}/{len(queries)} returned None (skipping)", flush=True)
            # 返回 count=0 表示查询失败但继续运行
            yield (query, 0, duration)
            continue
        if DEBUG:
            print(f"DEBUG CLIENT: Query {i+1}/{len(queries)} completed in {duration}us, count={count}", flush=True)
        yield (query, count, duration)

class Query(object):
    def __init__(self, query, tags):
        self.query = query
        self.tags = tags

def read_queries(query_path):
    for q in open(query_path):
        c = json.loads(q)
        yield Query(c["query"], c["tags"])

# Print progress, borrowed from https://stackoverflow.com/questions/3173320/text-progress-bar-in-terminal-with-block-characters
def printProgressBar (progress, prefix = '', suffix = '', decimals = 1, length = 100, fill = '█', printEnd = "\r"):
    """
    Call in a loop to create terminal progress bar
    @params:
        progress    - Required  : current progress in [0,1] (Float)
        prefix      - Optional  : prefix string (Str)
        suffix      - Optional  : suffix string (Str)
        decimals    - Optional  : positive number of decimals in percent complete (Int)
        length      - Optional  : character length of bar (Int)
        fill        - Optional  : bar fill character (Str)
        printEnd    - Optional  : end character (e.g. "\r", "\r\n") (Str)
    """
    percent = ("{0:." + str(decimals) + "f}").format(100 * progress)
    filledLength = int(length * progress)
    bar = fill * filledLength + '-' * (length - filledLength)
    print(f'\r{prefix} |{bar}| {percent}% {suffix}', end = printEnd)
    # Print New Line on Complete
    if progress >= 1:
        print()

WARMUP_TIME = int(os.environ.get('WARMUP_TIME', '60'))
NUM_ITER = int(os.environ.get('NUM_ITER', '10'))

if __name__ == "__main__":
    import sys
    random.seed(2)
    query_path = sys.argv[1]
    engines = sys.argv[2:]
    queries = list(read_queries(query_path))

    details = {}
    for engine in engines:
      dirname = os.path.split(os.path.abspath(__file__))[0]
      dirname = path.dirname(dirname)
      dirname = path.join(dirname, "engines")
      details_file = path.join(dirname, engine, "details.json")
      if os.path.exists(details_file):
        with open(details_file, "r") as f:
          details[engine] = json.loads(f.read())
      else:
        details[engine] = []

    results = {}
    for command in COMMANDS:
        results_commands = {}
        for engine in engines:
            engine_results = []
            query_idx = {}
            for query in queries:
                query_result = {
                    "query": query.query,
                    "tags": query.tags,
                    "count": 0,
                    "duration": []
                }
                query_idx[query.query] = query_result
                engine_results.append(query_result)
            print("======================")
            print("BENCHMARKING %s %s" % (engine, command))
            search_client = SearchClient(engine)
            queries_shuffled = list(queries[:])
            random.seed(2)
            random.shuffle(queries_shuffled)
            warmup_start = time.monotonic()
            warmup_iter = 0
            printProgressBar(0, prefix = 'Warmup:', suffix = 'Complete', length = 50)
            while True:
                warmup_iter += 1
                query_count = 0
                for _ in drive(queries_shuffled, search_client, command):
                    query_count += 1
                elapsed = time.monotonic() - warmup_start
                progress = min(1, elapsed / WARMUP_TIME)
                if DEBUG:
                    print(f"\nWarmup iter {warmup_iter}: {query_count} queries, elapsed {elapsed:.1f}s/{WARMUP_TIME}s", flush=True)
                printProgressBar(progress, prefix = 'Warmup:', suffix = 'Complete', length = 50)
                if progress == 1:
                    break
            printProgressBar(0, prefix = 'Run:   ', suffix = 'Complete', length = 50)
            for i in range(NUM_ITER):
                for (query, count, duration) in drive(queries_shuffled, search_client, command):
                    if count is None or count == 0:
                        # 查询失败，记录为无效结果
                        pass
                    else:
                        query_idx[query.query]["count"] = count
                        query_idx[query.query]["duration"].append(duration)
                printProgressBar(float(i + 1) / NUM_ITER, prefix = 'Run:   ', suffix = 'Complete', length = 50)
            for query in engine_results:
                query["duration"].sort()
            results_commands[engine] = engine_results
            search_client.close()
        print(results_commands.keys())
        results[command] = results_commands
    with open("results.json" , "w") as f:
        json.dump({ "details": details, "results": results }, f, default=lambda obj: obj.__dict__)
