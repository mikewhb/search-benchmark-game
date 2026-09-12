//! Attribute the per-query wall clock of a Lance FTS `TOP_10` to its phases.
//!
//! Diagnostic tool for the 1.20x-Lucene work: the SBG ruler only reports one
//! number per query, so it cannot say how much of a cheap query is spent
//! before any posting is touched. This replays one query and reports the
//! median/best microseconds of each phase of the same call sequence
//! `do_query.rs` uses.
//!
//! Usage: phase_timing <index-dir> <reps> <query>...

use std::env;
use std::path::Path;
use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use lance::dataset::builder::DatasetBuilder;
use lance::index::DatasetIndexExt;
use lance::Dataset;
use lance_datafusion::exec::{get_session_context, LanceExecutionOptions};
use lance_index::scalar::inverted::query::{FtsQuery, MatchQuery, Operator};
use lance_index::scalar::FullTextSearchQuery;

const INDEX_NAME: &str = "text_idx";

/// Time the two `SessionState` deep clones that `lance_datafusion::exec::
/// get_task_context` performs on every query, and the `Arc<TaskContext>`
/// reuse that would replace them. This bounds what removing the per-query
/// registry clones can win.
fn report_task_context_cost(reps: usize) {
    let options = LanceExecutionOptions {
        batch_size: Some(8192),
        ..Default::default()
    };
    let ctx = get_session_context(&options);
    for _ in 0..50 {
        let _ = ctx.state().task_ctx();
    }

    let mut current = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        // Exactly what get_task_context does today.
        let mut state = ctx.state();
        state.config_mut().options_mut().execution.batch_size = 8192;
        let task_ctx = state.task_ctx();
        current.push(t.elapsed().as_nanos());
        std::hint::black_box(task_ctx);
    }

    // Lower bound: the same value handed out from behind an Arc.
    let cached = ctx.state().task_ctx();
    let mut reuse = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        let task_ctx = std::sync::Arc::clone(&cached);
        reuse.push(t.elapsed().as_nanos());
        std::hint::black_box(task_ctx);
    }

    current.sort_unstable();
    reuse.sort_unstable();
    let cur = current[reps / 2] as f64 / 1000.0;
    let reu = reuse[reps / 2] as f64 / 1000.0;
    println!(
        "get_task_context (per query)   today {cur:>7.1}us | Arc reuse {reu:>7.3}us \
         | removable {:>7.1}us",
        cur - reu
    );
}

#[derive(Default, Clone, Copy)]
struct Phases {
    build_query: u128,
    scan: u128,
    configure: u128,
    create_plan: u128,
    execute_plan: u128,
    drain: u128,
    total: u128,
}

/// Mirrors `do_query.rs::run_search` for an all-SHOULD or all-MUST term query,
/// with a clock between each call so no phase is hidden inside another.
async fn timed_search(
    dataset: &Dataset,
    raw: &str,
    operator: Operator,
) -> Result<Phases, Box<dyn std::error::Error>> {
    let mut p = Phases::default();
    let t_total = Instant::now();

    let t = Instant::now();
    let query = FtsQuery::Match(
        MatchQuery::new(raw.to_string())
            .with_column(Some("text".to_string()))
            .with_operator(operator)
            .with_fuzziness(Some(0)),
    );
    let fts = FullTextSearchQuery::new_query(query).limit(Some(10));
    p.build_query = t.elapsed().as_nanos();

    let t = Instant::now();
    let mut scanner = dataset.scan();
    p.scan = t.elapsed().as_nanos();

    let t = Instant::now();
    scanner.empty_project()?;
    scanner.with_row_id();
    scanner.fast_search();
    scanner.full_text_search(fts)?;
    scanner.limit(Some(10), None)?;
    scanner.batch_size(8192);
    p.configure = t.elapsed().as_nanos();

    // Split what `try_into_stream` fuses: `create_plan` is pure plan
    // construction, which a lean retrieval path would drop entirely, while
    // `execute_plan` is session/task-context lookup plus the stream adapter.
    let t = Instant::now();
    let plan = scanner.create_plan().await?;
    p.create_plan = t.elapsed().as_nanos();

    let t = Instant::now();
    let mut stream = lance::dataset::scanner::DatasetRecordBatchStream::new(
        lance_datafusion::exec::execute_plan(
            plan,
            LanceExecutionOptions {
                batch_size: Some(8192),
                ..Default::default()
            },
        )?,
    );
    p.execute_plan = t.elapsed().as_nanos();

    let t = Instant::now();
    let mut rows = 0usize;
    while let Some(batch) = stream.try_next().await? {
        rows += batch.num_rows();
    }
    p.drain = t.elapsed().as_nanos();

    p.total = t_total.elapsed().as_nanos();
    let _ = rows;
    Ok(p)
}

fn summarize(label: &str, mut runs: Vec<Phases>) {
    let n = runs.len();
    macro_rules! stat {
        ($field:ident) => {{
            runs.sort_by_key(|p| p.$field);
            let best = runs[0].$field as f64 / 1000.0;
            let med = runs[n / 2].$field as f64 / 1000.0;
            (best, med)
        }};
    }
    let bq = stat!(build_query);
    let sc = stat!(scan);
    let cf = stat!(configure);
    let cp = stat!(create_plan);
    let ep = stat!(execute_plan);
    let dr = stat!(drain);
    let to = stat!(total);
    println!(
        "{label:<28} total {:>8.1} {:>8.1} | build_query {:>7.1} | scan {:>7.1} \
         | configure {:>7.1} | create_plan {:>8.1} | execute_plan {:>7.1} | drain {:>8.1}",
        to.0, to.1, bq.1, sc.1, cf.1, cp.1, ep.1, dr.1
    );
}

async fn run(
    index_dir: &Path,
    reps: usize,
    queries: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let uri = index_dir.to_str().ok_or("index dir is not utf-8")?;
    let dataset = DatasetBuilder::from_uri(uri).load().await?;
    if let Err(err) = dataset.prewarm_index(INDEX_NAME).await {
        eprintln!("prewarm_index skipped: {err}");
    }

    report_task_context_cost(reps);
    println!("(microseconds; `total best  total median`, then per-phase median)");
    for raw in queries {
        let (text, operator) = match raw.strip_prefix('+') {
            Some(rest) => (rest.replace('+', ""), Operator::And),
            None => (raw.clone(), Operator::Or),
        };
        for _ in 0..20 {
            timed_search(&dataset, &text, operator).await?;
        }
        let mut runs = Vec::with_capacity(reps);
        for _ in 0..reps {
            runs.push(timed_search(&dataset, &text, operator).await?);
        }
        summarize(raw, runs);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        return Err("usage: phase_timing <index-dir> <reps> <query>...".into());
    }
    let index_dir = Path::new(&args[1]).to_path_buf();
    let reps: usize = args[2].parse()?;
    let queries = args[3..].to_vec();
    let runtime = match env::var("LANCE_BENCH_RUNTIME").ok().as_deref() {
        Some("current_thread") => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
        _ => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    };
    runtime.block_on(run(&index_dir, reps, &queries))
}
