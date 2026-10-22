use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::app_error::AppCommandError;

const RRF_OFFSET: f64 = 60.0;

pub fn normalize(value: Value) -> Result<Value, AppCommandError> {
    let result = &value["result"];
    if result["code"].as_i64() != Some(0) {
        return Err(super::api::invalid_response());
    }
    let items = result["data"]["result_list"]
        .as_array()
        .ok_or_else(super::api::invalid_response)?;
    let count = result["data"]["count"]
        .as_u64()
        .ok_or_else(super::api::invalid_response)?;
    let results = items
        .iter()
        .map(|item| {
            if !item.is_object() {
                return Err(super::api::invalid_response());
            }
            let document = &item["doc_info"];
            Ok(json!({
                "id": item["id"], "score": item["score"], "content": item["content"],
                "md_content": item["md_content"], "chunk_type": item["chunk_type"],
                "doc_id": document["doc_id"], "doc_name": document["doc_name"],
                "doc_type": document["doc_type"],
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"count": count, "results": results}))
}

pub fn merge(groups: Vec<Value>, limit: usize) -> Value {
    let mut ranked: BTreeMap<String, (f64, Value)> = BTreeMap::new();
    for group in groups {
        let mut seen = BTreeSet::new();
        for (rank, item) in group["results"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let key = json!([
                item["category"],
                item["doc_id"],
                item["id"],
                item["content"]
            ])
            .to_string();
            if !seen.insert(key.clone()) {
                continue;
            }
            let entry = ranked.entry(key).or_insert_with(|| (0.0, item.clone()));
            entry.0 += 1.0 / (RRF_OFFSET + rank as f64 + 1.0);
        }
    }
    let mut ranked: Vec<_> = ranked.into_values().collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let results: Vec<_> = ranked
        .into_iter()
        .take(limit)
        .map(|(score, mut item)| {
            item["fusion_score"] = json!(score);
            item
        })
        .collect();
    json!({"count": results.len(), "results": results, "scope": "conversation_selection"})
}
