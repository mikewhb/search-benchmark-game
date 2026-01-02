use arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;
use lancedb::index::scalar::FtsIndexBuilder;
use lancedb::index::Index;
use std::env;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

// 每批处理的文档数量，避免 Arrow 的 offset overflow 问题
const BATCH_SIZE: usize = 100_000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    let output_dir = Path::new(&args[1]);
    
    // 定义 schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
    ]));
    
    // 连接到 LanceDB
    let db = connect(output_dir.to_str().unwrap()).execute().await?;
    
    // 删除已存在的表
    let tables = db.table_names().execute().await?;
    if tables.contains(&"wiki_articles".to_string()) {
        db.drop_table("wiki_articles", &[]).await?;
    }
    
    // 收集文档并分批处理
    let mut ids: Vec<String> = Vec::with_capacity(BATCH_SIZE);
    let mut texts: Vec<String> = Vec::with_capacity(BATCH_SIZE);
    
    let stdin = std::io::stdin();
    let mut total_count = 0;
    let mut batch_count = 0;
    let mut table = None;
    
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        
        let doc: serde_json::Value = serde_json::from_str(&line)?;
        ids.push(doc["id"].as_str().unwrap_or("").to_string());
        texts.push(doc["text"].as_str().unwrap_or("").to_string());
        total_count += 1;
        
        if total_count % 100_000 == 0 {
            println!("Processed: {}", total_count);
        }
        
        // 达到批次大小时写入
        if ids.len() >= BATCH_SIZE {
            let batch = create_batch(&schema, &mut ids, &mut texts)?;
            
            if table.is_none() {
                // 第一批：创建表
                let batches = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());
                table = Some(
                    db.create_table("wiki_articles", Box::new(batches))
                        .execute()
                        .await?
                );
                batch_count += 1;
                println!("Table created with batch {}", batch_count);
            } else {
                // 后续批次：追加数据
                let batches = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());
                table.as_ref().unwrap()
                    .add(Box::new(batches))
                    .execute()
                    .await?;
                batch_count += 1;
                println!("Added batch {}", batch_count);
            }
        }
    }
    
    // 处理剩余的文档
    if !ids.is_empty() {
        let batch = create_batch(&schema, &mut ids, &mut texts)?;
        
        if table.is_none() {
            let batches = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());
            table = Some(
                db.create_table("wiki_articles", Box::new(batches))
                    .execute()
                    .await?
            );
            batch_count += 1;
            println!("Table created with batch {}", batch_count);
        } else {
            let batches = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());
            table.as_ref().unwrap()
                .add(Box::new(batches))
                .execute()
                .await?;
            batch_count += 1;
            println!("Added final batch {}", batch_count);
        }
    }
    
    println!("Total documents: {}, Total batches: {}", total_count, batch_count);
    
    if let Some(tbl) = table {
        println!("Creating FTS index...");
        
        // 创建 FTS 索引 (LanceDB 0.23.0 API)
        tbl.create_index(
            &["text"],
            Index::FTS(FtsIndexBuilder::default().with_position(true))
        )
        .execute()
        .await?;
        
        println!("FTS index created successfully");
    } else {
        println!("No documents to index");
    }
    
    Ok(())
}

fn create_batch(
    schema: &Arc<Schema>,
    ids: &mut Vec<String>,
    texts: &mut Vec<String>,
) -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(std::mem::take(ids))),
            Arc::new(StringArray::from(std::mem::take(texts))),
        ],
    )?;
    Ok(batch)
}