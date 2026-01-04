use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::index::scalar::{
    FtsQuery, FullTextSearchQuery, MatchQuery,
    Operator, PhraseQuery};
use lancedb::Table;
use std::env;
use std::io::BufRead;
use std::path::Path;
use once_cell::sync::Lazy;
use regex::Regex;

// 预编译正则表达式
static INTERSECTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\+(\S+)").unwrap());

// 预分配的列选择
static ID_COLUMNS: Lazy<Select> = Lazy::new(|| Select::Columns(vec!["id".to_string()]));

/// 查询类型枚举
#[derive(Debug)]
enum QueryType {
    Term(String),           // 单个词
    Union(String),          // OR 查询 (word1 word2)
    Intersection(String),   // AND 查询 (+word1 +word2)
    Phrase(String),         // 短语查询 ("word1 word2")
}

/// 解析 Tantivy 风格的查询字符串
#[inline]
fn parse_query(query_str: &str) -> QueryType {
    let query_str = query_str.trim();
    
    // 检查是否是 phrase 查询 (被双引号包围)
    if query_str.starts_with('"') && query_str.ends_with('"') && query_str.len() > 2 {
        let inner = &query_str[1..query_str.len()-1];
        return QueryType::Phrase(inner.to_string());
    }
    
    // 检查是否是 intersection 查询 (所有词都有 + 前缀)
    if query_str.starts_with('+') {
        let terms: Vec<String> = INTERSECTION_RE.captures_iter(query_str)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        if !terms.is_empty() {
            // 检查整个查询是否只由 +term 组成
            let reconstructed = terms.iter()
                .map(|t| format!("+{}", t))
                .collect::<Vec<_>>()
                .join(" ");
            
            if reconstructed == query_str {
                return QueryType::Intersection(terms.join(" "));
            }
        }
    }
    
    // 检查是否是单个词 (term 查询)
    if !query_str.contains(' ') {
        return QueryType::Term(query_str.to_string());
    }
    
    // 默认是 union 查询 (多个词，用空格分隔)
    QueryType::Union(query_str.to_string())
}

/// 根据查询类型构建 FTS 查询
#[inline]
fn build_fts_query(query_str: &str) -> FullTextSearchQuery {
    let query_type = parse_query(query_str);
    
    let fts_query = match query_type {
        QueryType::Phrase(terms) => {
            FtsQuery::Phrase(PhraseQuery::new(terms))
        }
        QueryType::Intersection(terms) => {
            FtsQuery::Match(MatchQuery::new(terms).with_operator(Operator::And))
        }
        QueryType::Union(terms) => {
            FtsQuery::Match(MatchQuery::new(terms).with_operator(Operator::Or))
        }
        QueryType::Term(term) => {
            FtsQuery::Match(MatchQuery::new(term))
        }
    };
    
    FullTextSearchQuery::new_query(fts_query)
}

/// 执行查询并返回结果数量
#[inline]
async fn execute_count(table: &Table, query_str: &str, limit: usize) -> Result<usize, ()> {
    let fts_query = build_fts_query(query_str);
    let query_result = table
        .query()
        .full_text_search(fts_query)
        .select(ID_COLUMNS.clone())
        // .select(Select::columns(&["_score"]))  // 只选择 FTS 分数列
        // .with_row_id()
        // .fast_search()
        .limit(limit)
        .execute()
        .await;
    
    match query_result {
        Ok(results) => {
            use futures::StreamExt;
            let mut count = 0usize;
            futures::pin_mut!(results);
            while let Some(batch_result) = results.next().await {
                if let Ok(batch) = batch_result {
                    count += batch.num_rows();
                }
            }
            Ok(count)
        }
        Err(_) => Err(())
    }
}

/// 执行 TOP N 查询
#[inline]
async fn execute_top_n(table: &Table, query_str: &str, limit: usize) -> Result<(), ()> {
    let fts_query = build_fts_query(query_str);
    let query_result = table
        .query()
        .full_text_search(fts_query)
        .select(ID_COLUMNS.clone())
        // .select(Select::columns(&["_score"]))  // 只选择 FTS 分数列
        // .with_row_id()
        // .fast_search()
        .limit(limit)
        .execute()
        .await;
    
    match query_result {
        Ok(results) => {
            use futures::StreamExt;
            futures::pin_mut!(results);
            // 消费流但不需要收集
            while let Some(_) = results.next().await {}
            Ok(())
        }
        Err(_) => Err(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let index_dir = Path::new(&args[1]);
    
    // 连接到 LanceDB
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
        
        match command {
            "COUNT" => {
                match execute_count(&table, query_str, 1000000).await {
                    Ok(count) => println!("{}", count),
                    Err(_) => println!("UNSUPPORTED"),
                }
            }
            "TOP_10" => {
                match execute_top_n(&table, query_str, 10).await {
                    Ok(_) => println!("1"),
                    Err(_) => println!("UNSUPPORTED"),
                }
            }
            "TOP_100" => {
                match execute_top_n(&table, query_str, 100).await {
                    Ok(_) => println!("1"),
                    Err(_) => println!("UNSUPPORTED"),
                }
            }
            "TOP_1000" => {
                match execute_top_n(&table, query_str, 1000).await {
                    Ok(_) => println!("1"),
                    Err(_) => println!("UNSUPPORTED"),
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
                
                // 获取 top N (消费结果)
                if execute_top_n(&table, query_str, limit).await.is_err() {
                    println!("UNSUPPORTED");
                    continue;
                }
                
                // 获取总数
                match execute_count(&table, query_str, 100000).await {
                    Ok(count) => println!("{}", count),
                    Err(_) => println!("UNSUPPORTED"),
                }
            }
            _ => {
                println!("UNSUPPORTED");
            }
        }
    }
    
    Ok(())
}