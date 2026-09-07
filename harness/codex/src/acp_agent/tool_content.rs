use serde_json::{json, Map, Value};

pub(super) fn enrich(item: &Value, params: &mut Value) {
    match item["type"].as_str() {
        Some("fileChange") => file_changes(item, params),
        Some("imageGeneration") => generated_image(item, params),
        Some("imageView") => {
            if let Some(path) = item.get("path").and_then(Value::as_str) {
                params["locations"] = json!([{ "path": path }]);
                params["rawInput"] = json!({ "path": path });
                params["content"] = json!([content(
                    json!({ "type": "resource_link", "uri": path, "name": path })
                )]);
            }
        }
        Some("mcpToolCall") => {
            if let Some(values) = item.pointer("/result/content").and_then(Value::as_array) {
                params["content"] =
                    json!(values.iter().filter_map(mcp_content).collect::<Vec<_>>());
            }
            params["_meta"] = json!({ "is_mcp_tool_call": true });
        }
        Some("commandExecution") if item["status"] != "inProgress" => {
            params["rawOutput"] = json!({ "formatted_output": item.get("aggregatedOutput"), "exit_code": item.get("exitCode") });
        }
        _ => {}
    }
}

fn file_changes(item: &Value, params: &mut Value) {
    let Some(changes) = item.get("changes").and_then(Value::as_array) else {
        return;
    };
    let mut raw = Map::new();
    let mut locations = Vec::new();
    for change in changes {
        let (Some(path), Some(diff)) = (change["path"].as_str(), change["diff"].as_str()) else {
            continue;
        };
        let kind = change
            .pointer("/kind/type")
            .and_then(Value::as_str)
            .unwrap_or("update");
        let rendered = match kind {
            "add" => whole_file_diff(path, diff, true),
            "delete" => whole_file_diff(path, diff, false),
            _ => diff.to_string(),
        };
        raw.insert(path.into(), json!({ "diff": rendered }));
        locations.push(json!({ "path": path }));
        if let Some(destination) = change
            .pointer("/kind/move_path")
            .or_else(|| change.pointer("/kind/movePath"))
        {
            if destination.is_string() {
                locations.push(json!({ "path": destination }));
            }
        }
    }
    // 复用宿主已支持的 changes[path].diff，不读取磁盘重建可能已变化的文件。
    params["rawInput"] = json!({ "changes": raw });
    params["locations"] = json!(locations);
}

fn whole_file_diff(path: &str, text: &str, added: bool) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let (old, new, hunk, prefix) = if added {
        (
            "/dev/null".to_string(),
            format!("b/{path}"),
            format!("@@ -0,0 +1,{} @@", lines.len()),
            '+',
        )
    } else {
        (
            format!("a/{path}"),
            "/dev/null".to_string(),
            format!("@@ -1,{} +0,0 @@", lines.len()),
            '-',
        )
    };
    let mut diff = format!("--- {old}\n+++ {new}\n{hunk}\n");
    for line in lines {
        diff.push(prefix);
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn generated_image(item: &Value, params: &mut Value) {
    let mut values = Vec::new();
    if let Some(prompt) = item
        .get("revisedPrompt")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        values.push(content(json!({ "type": "text", "text": prompt })));
    }
    if let Some(data) = item
        .get("result")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        let mut image = json!({ "type": "image", "data": data, "mimeType": "image/png" });
        if let Some(path) = item.get("savedPath").filter(|path| path.is_string()) {
            image["uri"] = path.clone();
        }
        values.push(content(image));
    } else if let Some(path) = item.get("savedPath").and_then(Value::as_str) {
        values.push(content(
            json!({ "type": "resource_link", "uri": path, "name": path }),
        ));
    }
    params["content"] = json!(values);
    params["rawOutput"] = json!({ "status": item["status"], "savedPath": item.get("savedPath"), "failure": item.get("failure") });
    if item.get("failure").is_some_and(|value| !value.is_null()) {
        params["status"] = json!("failed");
    }
}

fn mcp_content(value: &Value) -> Option<Value> {
    // 通过实际 ACP schema 验证，避免未知 MCP 内容让整条工具更新无法反序列化。
    serde_json::from_value::<sacp::schema::ContentBlock>(value.clone()).ok()?;
    Some(content(value.clone()))
}

fn content(value: Value) -> Value {
    json!({ "type": "content", "content": value })
}
