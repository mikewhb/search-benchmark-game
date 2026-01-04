#![macro_use]
extern crate tantivy;

use tantivy::collector::{Count, TopDocs};
use tantivy::query::QueryParser;
use tantivy::tokenizer::TokenizerManager;
use tantivy::schema::Value;
use tantivy::{Index, DocAddress};

use std::env;
use std::io::BufRead;
use std::path::Path;
use std::fs::File;
use std::io::Write;
use chrono::Local;
use serde_json::json;

fn main() {
    let args: Vec<String> = env::args().collect();
    main_inner(Path::new(&args[1])).unwrap()
}

fn main_inner(index_dir: &Path) -> tantivy::Result<()> {
    // 日志文件路径 (当前目录下)
    let log_path = Path::new("check.log");
    let mut log_file = File::create(log_path).expect("Failed to create log file");
    
    writeln!(log_file, "# Tantivy 0.22 Check Log - {}", Local::now().format("%Y-%m-%dT%H:%M:%S")).unwrap();
    writeln!(log_file, "# Index: {}\n", index_dir.display()).unwrap();
    
    let index = Index::open_in_dir(index_dir).expect("failed to open index");
    let text_field = index.schema().get_field("text").expect("no text field?!");
    let id_field = index.schema().get_field("id").ok();
    let query_parser = QueryParser::new(
        index.schema(),
        vec![text_field],
        TokenizerManager::default(),
    );
    let reader = index.reader()?;
    let searcher = reader.searcher();

    let stdin = std::io::stdin();
    for line_res in stdin.lock().lines() {
        let line = line_res?;
        let fields: Vec<&str> = line.split("\t").collect();
        if fields.len() != 2 {
            println!("UNSUPPORTED");
            continue;
        }
        
        let command = fields[0];
        let query_str = fields[1];
        
        // 解析查询类型
        let query_type = if query_str.starts_with('"') && query_str.ends_with('"') {
            "phrase"
        } else if query_str.starts_with('+') {
            "intersection"
        } else if query_str.split_whitespace().count() > 1 {
            "union"
        } else {
            "term"
        };
        
        let query = match query_parser.parse_query(query_str) {
            Ok(q) => q,
            Err(e) => {
                writeln!(log_file, "ERROR: {} -> {:?}", query_str, e).unwrap();
                println!("UNSUPPORTED");
                continue;
            }
        };
        
        let actual_query = format!("{:?}", query);
        let count;
        let mut result_ids: Vec<String> = Vec::new();
        
        match command {
            "COUNT" => {
                count = query.count(&searcher)? as usize;
            }
            "TOP_10" => {
                let top_k = searcher.search(&query, &TopDocs::with_limit(10))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = 1;
            }
            "TOP_100" => {
                let top_k = searcher.search(&query, &TopDocs::with_limit(100))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = 1;
            }
            "TOP_1000" => {
                let top_k = searcher.search(&query, &TopDocs::with_limit(1000))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = 1;
            }
            "TOP_1_COUNT" => {
                let (top_k, count_) = searcher.search(&query, &(TopDocs::with_limit(1), Count))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = count_;
            }
            "TOP_5_COUNT" => {
                let (top_k, count_) = searcher.search(&query, &(TopDocs::with_limit(5), Count))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = count_;
            }
            "TOP_10_COUNT" => {
                let (top_k, count_) = searcher.search(&query, &(TopDocs::with_limit(10), Count))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = count_;
            }
            "TOP_100_COUNT" => {
                let (top_k, count_) = searcher.search(&query, &(TopDocs::with_limit(100), Count))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = count_;
            }
            "TOP_1000_COUNT" => {
                let (top_k, count_) = searcher.search(&query, &(TopDocs::with_limit(1000), Count))?;
                result_ids = get_doc_ids(&searcher, &top_k, id_field);
                count = count_;
            }
            _ => {
                println!("UNSUPPORTED");
                continue;
            }
        }
        
        let log_entry = json!({
            "timestamp": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            "original_query": query_str,
            "query_type": query_type,
            "actual_query": actual_query,
            "command": command,
            "result_count": count,
            "result_ids": result_ids
        });
        writeln!(log_file, "{}", log_entry).unwrap();
        
        println!("{}", count);
    }

    Ok(())
}

fn get_doc_ids(searcher: &tantivy::Searcher, top_docs: &[(f32, DocAddress)], id_field: Option<tantivy::schema::Field>) -> Vec<String> {
    let mut ids = Vec::new();
    for (_score, doc_address) in top_docs.iter().take(20) {
        if let Ok(doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
            if let Some(field) = id_field {
                if let Some(value) = doc.get_first(field) {
                    if let Some(text) = value.as_str() {
                        ids.push(text.to_string());
                    }
                }
            } else {
                ids.push(format!("{}:{}", doc_address.segment_ord, doc_address.doc_id));
            }
        }
    }
    ids
}
