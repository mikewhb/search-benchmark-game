//! Split Wikipedia FTS latency into plan vs execute, or loop for perf attach.
//!
//! Usage:
//!   probe_wiki <index-dir> [case-prefix]
//!   probe_wiki <index-dir> --query <sbg-query> [--query <sbg-query> ...]
//!   probe_wiki <index-dir> --queries-file <jsonl-or-lines> --seconds <n>

use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use futures::TryStreamExt;
use lance::dataset::builder::DatasetBuilder;
use lance::index::DatasetIndexExt;
use lance::Dataset;
use lance_index::scalar::inverted::query::{
    BooleanQuery, FtsQuery, MatchQuery, Occur, Operator, PhraseQuery,
};
use lance_index::scalar::FullTextSearchQuery;

const INDEX_NAME: &str = "text_idx";

fn match_query(terms: &str, op: Operator) -> FtsQuery {
    FtsQuery::Match(
        MatchQuery::new(terms.to_string())
            .with_column(Some("text".to_string()))
            .with_operator(op)
            .with_fuzziness(Some(0)),
    )
}

fn phrase_query(terms: &str) -> FtsQuery {
    FtsQuery::Phrase(PhraseQuery::new(terms.to_string()).with_column(Some("text".to_string())))
}

fn must_should(must: &str, should: &str) -> FtsQuery {
    FtsQuery::Boolean(BooleanQuery::new(vec![
        (Occur::Must, match_query(must, Operator::Or)),
        (Occur::Should, match_query(should, Operator::Or)),
    ]))
}

struct Case {
    name: &'static str,
    query: FtsQuery,
    limit: Option<i64>,
}

fn boolean_must(term: &str) -> FtsQuery {
    FtsQuery::Boolean(BooleanQuery::new(vec![(
        Occur::Must,
        match_query(term, Operator::Or),
    )]))
}

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
    let clauses = parse_clauses(raw.trim())?;
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

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "and_rare",
            query: match_query("griffith observatory", Operator::And),
            limit: Some(10),
        },
        Case {
            name: "or_rare",
            query: match_query("griffith observatory", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "phrase_rare",
            query: phrase_query("griffith observatory"),
            limit: Some(10),
        },
        Case {
            name: "term_climate",
            query: match_query("climate", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "must_only_climate",
            query: boolean_must("climate"),
            limit: Some(10),
        },
        Case {
            name: "term_policy",
            query: match_query("policy", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "must_should_climate",
            query: must_should("climate", "policy"),
            limit: Some(10),
        },
        Case {
            name: "must_should_climate_count",
            query: must_should("climate", "policy"),
            limit: None,
        },
        Case {
            name: "term_vehicles",
            query: match_query("vehicles", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "must_should_vehicles",
            query: must_should("vehicles", "electric"),
            limit: Some(10),
        },
        Case {
            name: "term_global",
            query: match_query("global", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "must_only_global",
            query: boolean_must("global"),
            limit: Some(10),
        },
        Case {
            name: "term_markets",
            query: match_query("markets", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "must_should_markets",
            query: must_should("global", "markets"),
            limit: Some(10),
        },
        Case {
            name: "must_should_privacy",
            query: must_should("data", "privacy"),
            limit: Some(10),
        },
        Case {
            name: "the_topk",
            query: match_query("the", Operator::Or),
            limit: Some(10),
        },
        Case {
            name: "iu_cheap_data_center",
            query: build_fts_query("+data center cooling").unwrap(),
            limit: Some(10),
        },
        Case {
            name: "iu_tail_customer_service",
            query: build_fts_query("customer +service phone number").unwrap(),
            limit: Some(10),
        },
        Case {
            name: "iu_tail_city_council",
            query: build_fts_query("city +council meeting agenda").unwrap(),
            limit: Some(10),
        },
        Case {
            name: "iu_tail_climate_policy",
            query: build_fts_query("+climate policy").unwrap(),
            limit: Some(10),
        },
        Case {
            name: "and_typical_new_york",
            query: match_query("new york", Operator::And),
            limit: Some(10),
        },
        Case {
            name: "phrase_typical_new_york",
            query: phrase_query("new york"),
            limit: Some(10),
        },
    ]
}

fn scanner(
    dataset: &Dataset,
    query: FtsQuery,
    limit: Option<i64>,
) -> lance::dataset::scanner::Scanner {
    let fts = FullTextSearchQuery::new_query(query).limit(limit);
    let mut scanner = dataset.scan();
    scanner.empty_project().unwrap();
    scanner.with_row_id();
    scanner.fast_search();
    scanner.full_text_search(fts).unwrap();
    scanner.limit(limit, None).unwrap();
    scanner.batch_size(8192);
    scanner
}

async fn time_case(
    dataset: &Dataset,
    case: &Case,
    warmup: usize,
    iters: usize,
) -> (f64, f64, usize) {
    for _ in 0..warmup {
        let mut stream = scanner(dataset, case.query.clone(), case.limit)
            .try_into_stream()
            .await
            .unwrap();
        while stream.try_next().await.unwrap().is_some() {}
    }

    let mut plan_ns = 0u128;
    let mut full_ns = 0u128;
    let mut rows = 0usize;
    for _ in 0..iters {
        let scan = scanner(dataset, case.query.clone(), case.limit);
        let t0 = Instant::now();
        let _plan = scan.create_plan().await.unwrap();
        plan_ns += t0.elapsed().as_nanos();

        let t2 = Instant::now();
        let mut stream = scanner(dataset, case.query.clone(), case.limit)
            .try_into_stream()
            .await
            .unwrap();
        let mut count = 0usize;
        while let Some(batch) = stream.try_next().await.unwrap() {
            count += batch.num_rows();
        }
        full_ns += t2.elapsed().as_nanos();
        rows = count;
    }
    let n = iters as f64;
    (
        plan_ns as f64 / n / 1000.0,
        full_ns as f64 / n / 1000.0,
        rows,
    )
}

async fn open_dataset(index_dir: &Path) -> Dataset {
    let uri = index_dir.to_str().expect("utf-8");
    let dataset = DatasetBuilder::from_uri(uri).load().await.unwrap();
    if let Err(err) = dataset.prewarm_index(INDEX_NAME).await {
        eprintln!("prewarm_index skipped: {err}");
    }
    dataset
}

async fn run_once(dataset: &Dataset, case: &Case) -> usize {
    let mut stream = scanner(dataset, case.query.clone(), case.limit)
        .try_into_stream()
        .await
        .unwrap();
    let mut count = 0usize;
    while let Some(batch) = stream.try_next().await.unwrap() {
        count += batch.num_rows();
    }
    count
}

fn case_from_query(raw: &str) -> Case {
    Case {
        name: Box::leak(raw.to_string().into_boxed_str()),
        query: build_fts_query(raw).unwrap_or_else(|err| panic!("query {raw}: {err}")),
        limit: Some(10),
    }
}

fn load_queries_file(path: &str) -> Vec<Case> {
    let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let raw = if line.starts_with('{') {
            let value: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|err| panic!("{path}:{}: {err}", lineno + 1));
            value
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or_else(|| panic!("{path}:{}: missing query", lineno + 1))
                .to_string()
        } else {
            line.to_string()
        };
        out.push(case_from_query(&raw));
    }
    out
}

struct Cli {
    index: String,
    case_prefix: Option<String>,
    extra_queries: Vec<String>,
    queries_file: Option<String>,
    seconds: Option<f64>,
    warmup: usize,
    iters: usize,
}

fn parse_cli(args: &[String]) -> Cli {
    if args.len() < 2 {
        eprintln!("usage: probe_wiki <index-dir> [case-prefix]");
        eprintln!("       probe_wiki <index-dir> --query <sbg-query> [--query <sbg-query> ...]");
        eprintln!("       probe_wiki <index-dir> --queries-file <jsonl-or-lines> --seconds <n>");
        std::process::exit(2);
    }
    let mut cli = Cli {
        index: args[1].clone(),
        case_prefix: None,
        extra_queries: Vec::new(),
        queries_file: None,
        seconds: None,
        warmup: 8,
        iters: 20,
    };
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--query" => {
                cli.extra_queries
                    .push(args.get(i + 1).cloned().unwrap_or_else(|| {
                        panic!("--query needs a value");
                    }));
                i += 2;
            }
            "--queries-file" => {
                cli.queries_file = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                    panic!("--queries-file needs a path");
                }));
                i += 2;
            }
            "--seconds" => {
                cli.seconds = Some(
                    args.get(i + 1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| panic!("--seconds needs a number")),
                );
                i += 2;
            }
            "--warmup" => {
                cli.warmup = args
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| panic!("--warmup needs an integer"));
                i += 2;
            }
            "--iters" => {
                cli.iters = args
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| panic!("--iters needs an integer"));
                i += 2;
            }
            flag if flag.starts_with('-') => {
                panic!("unknown flag {flag}");
            }
            other => {
                if cli.case_prefix.is_some() {
                    panic!("unexpected argument {other}");
                }
                cli.case_prefix = Some(other.to_string());
                i += 1;
            }
        }
    }
    cli
}

async fn profile_loop(dataset: &Dataset, selected: &[Case], warmup: usize, seconds: f64) {
    for case in selected {
        for _ in 0..warmup {
            let _ = run_once(dataset, case).await;
        }
    }
    eprintln!(
        "PROFILE_START cases={} warmup={} seconds={seconds}",
        selected.len(),
        warmup
    );
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    let mut iters = 0u64;
    let mut rows = 0usize;
    while Instant::now() < deadline {
        for case in selected {
            rows = run_once(dataset, case).await;
            iters += 1;
        }
    }
    eprintln!("PROFILE_END iters={iters} last_rows={rows}");
}

async fn async_main(args: Vec<String>) {
    let cli = parse_cli(&args);
    let dataset = open_dataset(Path::new(&cli.index)).await;
    let selected: Vec<Case> = if let Some(path) = &cli.queries_file {
        load_queries_file(path)
    } else if !cli.extra_queries.is_empty() {
        cli.extra_queries
            .iter()
            .map(|q| case_from_query(q))
            .collect()
    } else {
        cases()
            .into_iter()
            .filter(|case| {
                cli.case_prefix
                    .as_deref()
                    .is_none_or(|prefix| case.name.starts_with(prefix))
            })
            .collect()
    };
    if selected.is_empty() {
        panic!("no queries selected");
    }
    if let Some(seconds) = cli.seconds {
        profile_loop(&dataset, &selected, cli.warmup, seconds).await;
        return;
    }
    println!(
        "{:<32} {:>10} {:>10} {:>10} {:>8}",
        "case", "plan_us", "search_us", "full_us", "rows"
    );
    for case in &selected {
        let (plan, full, rows) = time_case(&dataset, case, cli.warmup, cli.iters).await;
        println!(
            "{:<32} {plan:10.1} {:10.1} {full:10.1} {rows:8}",
            case.name,
            full - plan
        );
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let runtime = match env::var("LANCE_BENCH_RUNTIME").ok().as_deref() {
        Some("multi_thread") => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"),
        _ => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"),
    };
    runtime.block_on(async_main(args));
}
