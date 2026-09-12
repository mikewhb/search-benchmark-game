use std::env;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;

use arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lance::dataset::{WriteMode, WriteParams};
use lance::index::DatasetIndexExt;
use lance::Dataset;
use lance_index::scalar::inverted::tokenizer::Language;
use lance_index::scalar::InvertedIndexParams;
use lance_index::IndexType;

const BATCH_SIZE: usize = 50_000;
const INDEX_NAME: &str = "text_idx";

fn log(msg: &str) {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "{msg}");
    let _ = err.flush();
}

fn fts_params() -> InvertedIndexParams {
    InvertedIndexParams::new("simple".to_string(), Language::English)
        .with_position(true)
        .stem(false)
        .remove_stop_words(false)
        .ascii_folding(true)
        .lower_case(true)
}

fn take_batch(schema: &Arc<Schema>, ids: &mut Vec<String>, texts: &mut Vec<String>) -> RecordBatch {
    RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(std::mem::take(ids))),
            Arc::new(StringArray::from(std::mem::take(texts))),
        ],
    )
    .expect("failed to build record batch")
}

async fn write_corpus(output_dir: &Path) -> Result<Dataset, Box<dyn std::error::Error>> {
    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir)?;
    }
    std::fs::create_dir_all(output_dir)?;

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
    ]));
    let write_params = WriteParams {
        mode: WriteMode::Create,
        max_rows_per_file: 1_000_000,
        ..Default::default()
    };

    let mut ids = Vec::with_capacity(BATCH_SIZE);
    let mut texts = Vec::with_capacity(BATCH_SIZE);
    let mut dataset: Option<Dataset> = None;
    let mut total = 0usize;
    let stdin = std::io::stdin();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let doc: serde_json::Value = serde_json::from_str(&line)?;
        ids.push(doc["id"].as_str().unwrap_or("").to_string());
        texts.push(doc["text"].as_str().unwrap_or("").to_string());
        total += 1;
        if total % 100_000 == 0 {
            log(&format!("wrote {total} documents"));
        }
        if ids.len() >= BATCH_SIZE {
            let batch = take_batch(&schema, &mut ids, &mut texts);
            let reader = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());
            if dataset.is_none() {
                dataset = Some(
                    Dataset::write(
                        reader,
                        output_dir.to_str().unwrap(),
                        Some(write_params.clone()),
                    )
                    .await?,
                );
            } else {
                dataset
                    .as_mut()
                    .unwrap()
                    .append(reader, Some(write_params.clone()))
                    .await?;
            }
        }
    }

    if !ids.is_empty() {
        let batch = take_batch(&schema, &mut ids, &mut texts);
        let reader = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());
        if dataset.is_none() {
            dataset = Some(
                Dataset::write(
                    reader,
                    output_dir.to_str().unwrap(),
                    Some(write_params.clone()),
                )
                .await?,
            );
        } else {
            dataset
                .as_mut()
                .unwrap()
                .append(reader, Some(write_params))
                .await?;
        }
    }

    let dataset = dataset.ok_or("no documents on stdin")?;
    log(&format!("data load complete: {total} documents"));
    Ok(dataset)
}

async fn create_fts(dataset: &mut Dataset) -> Result<(), Box<dyn std::error::Error>> {
    log("creating inverted index (stem=false, stopwords=false, with_position=true)");
    dataset
        .create_index(
            &["text"],
            IndexType::Inverted,
            Some(INDEX_NAME.to_string()),
            &fts_params(),
            true,
        )
        .await?;
    log("inverted index created");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err("usage: build_index <index-dir>".into());
    }
    let output_dir = Path::new(&args[1]);
    let index_only = env::var("LANCE_BENCH_INDEX_ONLY").ok().as_deref() == Some("1");

    let mut dataset = if index_only {
        Dataset::open(output_dir.to_str().unwrap()).await?
    } else {
        write_corpus(output_dir).await?
    };
    create_fts(&mut dataset).await?;
    Ok(())
}
