use serde_json::Value;
use std::collections::BTreeMap;

const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 4096;
const MAX_ID_BYTES: usize = 64;

pub(super) fn requests(output: &str, image: bool) -> BTreeMap<String, &'static str> {
    let mut reader = Reader {
        image: image.then_some("image"),
        remaining: MAX_NODES,
        result: BTreeMap::new(),
    };
    reader.read(output, 0);
    reader.result
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn billing(value: &Value, result: &mut BTreeMap<String, &'static str>) {
    let Some(receipt) = value.pointer("/_meta/billing") else {
        return;
    };
    if matches!(
        receipt["outcome"].as_str(),
        Some("failed" | "cancelled" | "canceled")
    ) {
        return;
    }
    // 网关 call_id 与 Fusion request_id 不同；只读查询负责关联真实账本。
    if let Some(id) = receipt["call_id"].as_str().filter(|id| valid_id(id)) {
        result.insert(format!("tool-call:{id}"), "tool");
    } else if let Some(id) = receipt["request_id"].as_str().filter(|id| valid_id(id)) {
        result.insert(id.into(), "tool");
    }
}

struct Reader {
    image: Option<&'static str>,
    remaining: usize,
    result: BTreeMap<String, &'static str>,
}

impl Reader {
    fn read(&mut self, text: &str, depth: usize) {
        if text.len() > MAX_RESULT_BYTES || depth > MAX_DEPTH || self.remaining == 0 {
            return;
        }
        // 远山会把多个文本块以换行拼接，可能同时包含业务文本和最小计费回执。
        for value in serde_json::Deserializer::from_str(text)
            .into_iter::<Value>()
            .take(MAX_NODES)
        {
            let Ok(value) = value else { break };
            self.visit(&value, depth);
        }
        for line in text
            .lines()
            .filter(|line| line.trim_start().starts_with("{\"_meta\":"))
        {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                billing(&value, &mut self.result);
            }
        }
    }

    fn visit(&mut self, value: &Value, depth: usize) {
        if depth > MAX_DEPTH || self.remaining == 0 {
            return;
        }
        self.remaining -= 1;
        billing(value, &mut self.result);
        let image = self.image_purpose(value);
        if let Some(purpose) = image {
            if let Some(id) = value
                .get("request_id")
                .and_then(Value::as_str)
                .filter(|id| valid_id(id))
            {
                self.result.entry(id.into()).or_insert(purpose);
            }
        }
        let previous = self.image;
        self.image = image;
        self.visit_children(value, depth);
        self.image = previous;
    }

    fn image_purpose(&self, value: &Value) -> Option<&'static str> {
        let operation = value
            .get("operation")
            .or_else(|| value.get("type"))
            .and_then(Value::as_str);
        if self.image.is_none() && value.get("images").is_none() {
            return None;
        }
        match operation {
            Some("generate") => Some("image_generation"),
            Some("edit") => Some("image_editing"),
            Some("batch") => Some("image"),
            _ => self.image,
        }
    }

    fn visit_children(&mut self, value: &Value, depth: usize) {
        match value {
            Value::Array(items) => {
                for item in items.iter().take(MAX_NODES) {
                    self.visit(item, depth + 1);
                }
            }
            Value::Object(object) => {
                for key in [
                    "structuredContent",
                    "structured_content",
                    "content",
                    "metadata",
                    "items",
                    "runs",
                    "attempts",
                    "error",
                    "error_details",
                    "data",
                    "result",
                ] {
                    if let Some(child) = object.get(key) {
                        self.visit(child, depth + 1);
                    }
                }
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    self.read(text, depth + 1);
                }
            }
            _ => {}
        }
    }
}
