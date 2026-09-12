use std::env;
use std::io::{BufRead, Write};
use std::path::Path;

use arrow_array::{Array, Float32Array, UInt64Array};
use futures::TryStreamExt;
use lance::dataset::builder::DatasetBuilder;
use lance::index::DatasetIndexExt;
use lance::Dataset;
use lance_index::scalar::inverted::query::{
    BooleanQuery, FtsQuery, MatchQuery, Occur, Operator, PhraseQuery,
};
use lance_index::scalar::FullTextSearchQuery;

const INDEX_NAME: &str = "text_idx";

fn parse_clauses(query: &str) -> Result<Vec<(Occur, FtsQuery)>, String> {
    let mut clauses = Vec::new();
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let occur = if chars[i] == '+' {
            i += 1;
            Occur::Must
        } else {
            Occur::Should
        };
        if i >= chars.len() {
            return Err("trailing +".to_string());
        }
        if chars[i] == '"' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i >= chars.len() {
                return Err("unclosed quote".to_string());
            }
            let terms: String = chars[start..i].iter().collect();
            i += 1;
            clauses.push((
                occur,
                FtsQuery::Phrase(PhraseQuery::new(terms).with_column(Some("text".to_string()))),
            ));
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            let terms: String = chars[start..i].iter().collect();
            if terms.is_empty() {
                return Err("empty term".to_string());
            }
            clauses.push((
                occur,
                FtsQuery::Match(
                    MatchQuery::new(terms)
                        .with_column(Some("text".to_string()))
                        .with_fuzziness(Some(0)),
                ),
            ));
        }
    }
    if clauses.is_empty() {
        return Err("empty query".to_string());
    }
    Ok(clauses)
}

fn build_fts_query(raw: &str) -> Result<FtsQuery, String> {
    let query = raw.trim();
    let clauses = parse_clauses(query)?;
    if clauses.len() == 1 {
        return Ok(clauses.into_iter().next().unwrap().1);
    }

    let all_must = clauses
        .iter()
        .all(|(occur, _)| matches!(occur, Occur::Must));
    let all_should = clauses
        .iter()
        .all(|(occur, _)| matches!(occur, Occur::Should));
    let all_terms = clauses.iter().all(|(_, q)| matches!(q, FtsQuery::Match(_)));

    if all_terms && (all_must || all_should) {
        let terms = clauses
            .iter()
            .map(|(_, q)| match q {
                FtsQuery::Match(m) => m.terms.as_str(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let operator = if all_must {
            Operator::And
        } else {
            Operator::Or
        };
        return Ok(FtsQuery::Match(
            MatchQuery::new(terms)
                .with_column(Some("text".to_string()))
                .with_operator(operator)
                .with_fuzziness(Some(0)),
        ));
    }

    Ok(FtsQuery::Boolean(BooleanQuery::new(clauses)))
}

async fn run_search(
    dataset: &Dataset,
    query: FtsQuery,
    limit: Option<i64>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let fts = FullTextSearchQuery::new_query(query).limit(limit);
    let mut scanner = dataset.scan();
    scanner.empty_project()?;
    scanner.with_row_id();
    scanner.fast_search();
    scanner.full_text_search(fts)?;
    scanner.limit(limit, None)?;
    scanner.batch_size(8192);
    let mut stream = scanner.try_into_stream().await?;
    let mut count = 0usize;
    while let Some(batch) = stream.try_next().await? {
        count += batch.num_rows();
    }
    Ok(count)
}

/// Return the ranked hits as `row_id:score_bits` so two builds can be compared
/// for bit-level identity. `TOP_10` only ever reports the constant 1, so it
/// cannot witness a change in the result set.
async fn run_search_dump(
    dataset: &Dataset,
    query: FtsQuery,
    limit: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    let fts = FullTextSearchQuery::new_query(query).limit(Some(limit));
    let mut scanner = dataset.scan();
    scanner.empty_project()?;
    scanner.with_row_id();
    scanner.fast_search();
    scanner.full_text_search(fts)?;
    scanner.limit(Some(limit), None)?;
    scanner.batch_size(8192);
    let mut stream = scanner.try_into_stream().await?;
    let mut out = Vec::new();
    while let Some(batch) = stream.try_next().await? {
        let row_ids = batch
            .column_by_name("_rowid")
            .ok_or("scan did not return _rowid")?
            .as_any()
            .downcast_ref::<UInt64Array>()
            .ok_or("_rowid is not UInt64")?;
        let scores = batch
            .column_by_name("_score")
            .ok_or("scan did not return _score")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or("_score is not Float32")?;
        for i in 0..batch.num_rows() {
            out.push(format!(
                "{}:{:08x}",
                row_ids.value(i),
                scores.value(i).to_bits()
            ));
        }
    }
    Ok(out.join(","))
}

async fn handle_dump(
    dataset: &Dataset,
    command: &str,
    query: &str,
) -> Option<Result<String, Box<dyn std::error::Error>>> {
    let limit = match command {
        "TOP_10_DUMP" => 10,
        "TOP_100_DUMP" => 100,
        _ => return None,
    };
    let fts_query = match build_fts_query(query) {
        Ok(q) => q,
        Err(_) => return None,
    };
    Some(run_search_dump(dataset, fts_query, limit).await)
}

async fn handle_command(
    dataset: &Dataset,
    command: &str,
    query: &str,
) -> Result<Option<usize>, Box<dyn std::error::Error>> {
    let fts_query = match build_fts_query(query) {
        Ok(q) => q,
        Err(_) => return Ok(None),
    };
    match command {
        "COUNT" | "UNOPTIMIZED_COUNT" => Ok(Some(run_search(dataset, fts_query, None).await?)),
        "TOP_10" => {
            let _ = run_search(dataset, fts_query, Some(10)).await?;
            Ok(Some(1))
        }
        "TOP_100" => {
            let _ = run_search(dataset, fts_query, Some(100)).await?;
            Ok(Some(1))
        }
        "TOP_1000" => {
            let _ = run_search(dataset, fts_query, Some(1000)).await?;
            Ok(Some(1))
        }
        "TOP_1_COUNT" | "TOP_5_COUNT" | "TOP_10_COUNT" | "TOP_100_COUNT" | "TOP_1000_COUNT" => {
            Ok(Some(run_search(dataset, fts_query, None).await?))
        }
        _ => Ok(None),
    }
}

fn parse_bytes_env(name: &str) -> Result<Option<usize>, String> {
    let Ok(raw) = env::var(name) else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let (num, scale) = match raw.as_bytes().last().copied() {
        Some(b @ (b'k' | b'K' | b'm' | b'M' | b'g' | b'G')) => {
            let n = raw[..raw.len() - 1].trim();
            let scale = match b {
                b'k' | b'K' => 1024usize,
                b'm' | b'M' => 1024 * 1024,
                _ => 1024 * 1024 * 1024,
            };
            (n, scale)
        }
        _ => (raw, 1usize),
    };
    let value = num
        .parse::<usize>()
        .map_err(|err| format!("{name}={raw}: {err}"))?
        .checked_mul(scale)
        .ok_or_else(|| format!("{name}={raw}: overflow"))?;
    Ok(Some(value))
}

async fn open_dataset(index_dir: &Path) -> Result<Dataset, Box<dyn std::error::Error>> {
    let uri = index_dir.to_str().ok_or("index dir is not utf-8")?;
    let mut builder = DatasetBuilder::from_uri(uri);
    if let Some(bytes) = parse_bytes_env("LANCE_BENCH_INDEX_CACHE_BYTES")? {
        eprintln!("index_cache_size_bytes={bytes}");
        builder = builder.with_index_cache_size_bytes(bytes);
    }
    if let Some(bytes) = parse_bytes_env("LANCE_BENCH_METADATA_CACHE_BYTES")? {
        eprintln!("metadata_cache_size_bytes={bytes}");
        builder = builder.with_metadata_cache_size_bytes(bytes);
    }
    Ok(builder.load().await?)
}

async fn serve(index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dataset = open_dataset(index_dir).await?;
    // Always call prewarm_index (same as the tmpfs run). It is not equivalent
    // to Lucene/Tantivy mmap: it decodes postings into QuickCache and flips
    // planner flags (resident row_id path, non-staged compound coordinator).
    // Skipping it leaves those flags false for the whole process; client.py
    // warmup cannot set them. On a 2G box Strict prewarm will likely fail
    // because the invert does not fit; docs.prewarm() still ran, and we
    // continue. LANCE_BENCH_SKIP_PREWARM is only an emergency override.
    if env::var_os("LANCE_BENCH_SKIP_PREWARM").is_some() {
        eprintln!("prewarm_index skipped: LANCE_BENCH_SKIP_PREWARM");
    } else if let Err(err) = dataset.prewarm_index(INDEX_NAME).await {
        eprintln!("prewarm_index skipped: {err}");
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let mut fields = line.splitn(2, '\t');
        let command = fields.next().unwrap_or("");
        let query = fields.next().unwrap_or("");
        if command.is_empty() || query.is_empty() {
            writeln!(stdout, "UNSUPPORTED")?;
            stdout.flush()?;
            continue;
        }
        if let Some(dumped) = handle_dump(&dataset, command, query).await {
            match dumped {
                Ok(rows) => writeln!(stdout, "{rows}")?,
                Err(err) => {
                    eprintln!("dump failed command={command} query={query}: {err}");
                    writeln!(stdout, "UNSUPPORTED")?;
                }
            }
            stdout.flush()?;
            continue;
        }
        match handle_command(&dataset, command, query).await {
            Ok(Some(count)) => writeln!(stdout, "{count}")?,
            Ok(None) => writeln!(stdout, "UNSUPPORTED")?,
            Err(err) => {
                eprintln!("query failed command={command} query={query}: {err}");
                writeln!(stdout, "UNSUPPORTED")?;
            }
        }
        stdout.flush()?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err("usage: do_query <index-dir>".into());
    }
    let index_dir = Path::new(&args[1]).to_path_buf();
    let runtime = match env::var("LANCE_BENCH_RUNTIME").ok().as_deref() {
        Some("current_thread") => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
        _ => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    };
    runtime.block_on(serve(&index_dir))
}
