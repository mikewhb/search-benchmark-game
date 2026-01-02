use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::index::scalar::{
    FtsQuery, FullTextSearchQuery, MatchQuery, 
    Operator, PhraseQuery};
use std::env;
use std::io::BufRead;
use std::path::Path;
use std::fs::File;
use std::io::Write;
use regex::Regex;
use chrono::Local;
use serde_json::json;
use arrow_array::Array;

/// 查询类型枚举
#[derive(Debug, Clone)]
enum QueryType {
    Term(String),
    Union(String),
    Intersection(String),
    Phrase(String),
}

impl QueryType {
    fn name(&self) -> &'static str {
        match self {
            QueryType::Term(_) => "term",
            QueryType::Union(_) => "union",
            QueryType::Intersection(_) => "intersection",
            QueryType::Phrase(_) => "phrase",
        }
    }
    
    fn terms(&self) -> &str {
        match self {
            QueryType::Term(s) | QueryType::Union(s) | QueryType::Intersection(s) | QueryType::Phrase(s) => s,
        }
    }
}

/// 解析 Tantivy 风格的查询字符串
fn parse_query(query_str: &str) -> QueryType {
    let query_str = query_str.trim();
    
    if query_str.starts_with('"') && query_str.ends_with('"') && query_str.len() > 2 {
        let inner = &query_str[1..query_str.len()-1];
        return QueryType::Phrase(inner.to_string());
    }
    
    if query_str.starts_with('+') {
        let re = Regex::new(r"\+(\S+)").unwrap();
        let terms: Vec<String> = re.captures_iter(query_str)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        if !terms.is_empty() {
            let reconstructed = terms.iter()
                .map(|t| format!("+{}", t))
                .collect::<Vec<_>>()
                .join(" ");
            
            if reconstructed == query_str {
                return QueryType::Intersection(terms.join(" "));
            }
        }
    }
    
    let words: Vec<&str> = query_str.split_whitespace().collect();
    if words.len() == 1 {
        return QueryType::Term(query_str.to_string());
    }
    
    QueryType::Union(query_str.to_string())
}

/// 根据查询类型构建 FTS 查询
/// 
/// 使用 lance-index 的底层查询类型:
/// - Term/Union: MatchQuery with Operator::Or
/// - Intersection: MatchQuery with Operator::And  
/// - Phrase: PhraseQuery
fn build_fts_query(query_type: &QueryType) -> FullTextSearchQuery {
    let fts_query = match query_type {
        QueryType::Phrase(terms) => {
            // Phrase查询：要求词序匹配
            let phrase_query = PhraseQuery::new(terms.clone());
            FtsQuery::Phrase(phrase_query)
        }
        QueryType::Intersection(terms) => {
            // Intersection (AND)查询：所有term必须同时出现
            let match_query = MatchQuery::new(terms.clone())
                .with_operator(Operator::And);
            FtsQuery::Match(match_query)
        }
        QueryType::Union(terms) => {
            // Union (OR)查询：至少一个term出现
            let match_query = MatchQuery::new(terms.clone())
                .with_operator(Operator::Or);
            FtsQuery::Match(match_query)
        }
        QueryType::Term(term) => {
            // 单个词查询
            let match_query = MatchQuery::new(term.clone());
            FtsQuery::Match(match_query)
        }
    };

    FullTextSearchQuery::new_query(fts_query)
}

/// 获取实际查询的描述
fn get_actual_query_desc(query_type: &QueryType) -> String {
    match query_type {
        QueryType::Phrase(terms) => format!("PhraseQuery: \"{}\"", terms),
        QueryType::Intersection(terms) => format!("MatchQuery(AND): \"{}\"", terms),
        QueryType::Union(terms) => format!("MatchQuery(OR): \"{}\"", terms),
        QueryType::Term(term) => format!("MatchQuery: \"{}\"", term),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let index_dir = Path::new(&args[1]);
    
    // 日志文件路径 (当前目录下)
    let log_path = Path::new("check.log");
    let mut log_file = File::create(log_path)?;
    
    writeln!(log_file, "# LanceDB Rust FTS Check Log - {}", Local::now().format("%Y-%m-%dT%H:%M:%S"))?;
    writeln!(log_file, "# Index: {}\n", index_dir.display())?;
    
    let db = connect(index_dir.to_str().unwrap()).execute().await?;
    let table = db.open_table("wiki_articles").execute().await?;
    
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.split('\t').collect();
        
        if fields.len() != 2 {
            println!("UNSUPPORTED");
            continue;
        }
        
        let command = fields[0];
        let query_str = fields[1];
        let query_type = parse_query(query_str);
        let actual_query_desc = get_actual_query_desc(&query_type);
        
        match command {
            "COUNT" | "TOP_10" | "TOP_100" | "TOP_1000" => {
                let limit = match command {
                    "COUNT" => 1000000,
                    "TOP_10" => 10,
                    "TOP_100" => 100,
                    "TOP_1000" => 1000,
                    _ => 10,
                };
                
                let fts_query = build_fts_query(&query_type);
                let query_result = table
                    .query()
                    .full_text_search(fts_query)
                    .select(Select::Columns(vec!["id".to_string()]))
                    .limit(limit)
                    .execute()
                    .await;
                
                // 处理查询执行错误（如 phrase 查询但索引没有 position）
                let (count, result_ids, errors) = match query_result {
                    Ok(results) => {
                        let batches: Vec<_> = futures::StreamExt::collect::<Vec<_>>(results).await;
                        
                        // 收集错误信息
                        let mut errors: Vec<String> = Vec::new();
                        for batch_result in &batches {
                            if let Err(e) = batch_result {
                                errors.push(e.to_string());
                            }
                        }
                        
                        let count: usize = batches.iter()
                            .filter_map(|r| r.as_ref().ok())
                            .map(|b| b.num_rows())
                            .sum();
                        
                        // 获取部分结果ID
                        let mut result_ids: Vec<String> = Vec::new();
                        for batch in batches.iter().filter_map(|r| r.as_ref().ok()) {
                            if let Some(id_col) = batch.column_by_name("id") {
                                if let Some(arr) = id_col.as_any().downcast_ref::<arrow_array::StringArray>() {
                                    for i in 0..std::cmp::min(20 - result_ids.len(), arr.len()) {
                                        if !arr.is_null(i) {
                                            result_ids.push(arr.value(i).to_string());
                                        }
                                    }
                                }
                            }
                            if result_ids.len() >= 20 { break; }
                        }
                        
                        (count, result_ids, errors)
                    }
                    Err(e) => {
                        // 查询执行失败（如 phrase 查询但索引没有 position）
                        (0, Vec::new(), vec![e.to_string()])
                    }
                };
                
                let log_entry = json!({
                    "timestamp": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                    "original_query": query_str,
                    "query_type": query_type.name(),
                    "actual_query": actual_query_desc,
                    "command": command,
                    "result_count": count,
                    "result_ids": result_ids,
                    "errors": errors
                });
                writeln!(log_file, "{}", log_entry)?;
                
                // 如果有错误，输出 UNSUPPORTED 让 client 识别为 fail
                if !errors.is_empty() {
                    println!("UNSUPPORTED");
                } else if command == "COUNT" {
                    println!("{}", count);
                } else {
                    println!("{}", if count > 0 { 1 } else { 1 });
                }
            }
            "TOP_1_COUNT" | "TOP_5_COUNT" | "TOP_10_COUNT" | "TOP_100_COUNT" | "TOP_1000_COUNT" => {
                let limit = match command {
                    "TOP_1_COUNT" => 1,
                    "TOP_5_COUNT" => 5,
                    "TOP_10_COUNT" => 10,
                    "TOP_100_COUNT" => 100,
                    "TOP_1000_COUNT" => 1000,
                    _ => 10,
                };
                
                let fts_query = build_fts_query(&query_type);
                let query_result = table
                    .query()
                    .full_text_search(fts_query)
                    .select(Select::Columns(vec!["id".to_string()]))
                    .limit(limit)
                    .execute()
                    .await;
                
                // 处理查询执行错误
                let (count, result_ids, errors) = match query_result {
                    Ok(results) => {
                        let batches: Vec<_> = futures::StreamExt::collect::<Vec<_>>(results).await;
                        
                        // 收集错误信息
                        let mut errors: Vec<String> = Vec::new();
                        for batch_result in &batches {
                            if let Err(e) = batch_result {
                                errors.push(e.to_string());
                            }
                        }
                        
                        // 获取部分结果ID
                        let mut result_ids: Vec<String> = Vec::new();
                        for batch in batches.iter().filter_map(|r| r.as_ref().ok()) {
                            if let Some(id_col) = batch.column_by_name("id") {
                                if let Some(arr) = id_col.as_any().downcast_ref::<arrow_array::StringArray>() {
                                    for i in 0..std::cmp::min(20 - result_ids.len(), arr.len()) {
                                        if !arr.is_null(i) {
                                            result_ids.push(arr.value(i).to_string());
                                        }
                                    }
                                }
                            }
                            if result_ids.len() >= 20 { break; }
                        }
                        
                        // 获取总数
                        let fts_query2 = build_fts_query(&query_type);
                        let count_result = table
                            .query()
                            .full_text_search(fts_query2)
                            .select(Select::Columns(vec!["id".to_string()]))
                            .limit(100000)
                            .execute()
                            .await;
                        
                        let count = match count_result {
                            Ok(count_results) => {
                                let count_batches: Vec<_> = futures::StreamExt::collect::<Vec<_>>(count_results).await;
                                count_batches.iter()
                                    .filter_map(|r| r.as_ref().ok())
                                    .map(|b| b.num_rows())
                                    .sum()
                            }
                            Err(e) => {
                                errors.push(format!("count_query: {}", e));
                                0
                            }
                        };
                        
                        (count, result_ids, errors)
                    }
                    Err(e) => {
                        (0, Vec::new(), vec![e.to_string()])
                    }
                };
                
                let log_entry = json!({
                    "timestamp": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                    "original_query": query_str,
                    "query_type": query_type.name(),
                    "actual_query": actual_query_desc,
                    "command": command,
                    "result_count": count,
                    "result_ids": result_ids,
                    "errors": errors
                });
                writeln!(log_file, "{}", log_entry)?;
                
                // 如果有错误，输出 UNSUPPORTED 让 client 识别为 fail
                if !errors.is_empty() {
                    println!("UNSUPPORTED");
                } else {
                    println!("{}", count);
                }
            }
            _ => {
                println!("UNSUPPORTED");
            }
        }
    }
    
    Ok(())
}