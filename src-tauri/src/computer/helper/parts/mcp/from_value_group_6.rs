// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl ToolCallResult {
    pub(in crate::computer::helper::mcp) fn from_value(value: Value) -> Result<Self, McpError> {
        let Value::Object(mut map) = value else {
            return Err(McpError::Protocol(
                "a tools/call result that is not an object".into(),
            ));
        };
        let content = match map.remove("content") {
            Some(Value::Array(items)) => items,
            Some(Value::Null) | None => Vec::new(),
            Some(_) => return Err(McpError::Protocol("`content` is not an array".into())),
        };
        Ok(Self {
            is_error: map.get("isError").and_then(Value::as_bool).unwrap_or(false),
            content,
            structured: map.remove("structuredContent").filter(|v| !v.is_null()),
        })
    }

    /// Every text block, joined — the driver's own words for a refusal.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|c| c.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The driver's machine-readable code for a refused call, from any of the
    /// shapes it writes one in: `{code, …}`,
    /// `{status: "refused", refusal: {code, …}}`, or — on an action it
    /// answers without an error, `effect: "refused"` — `{error: {code, …}}`.
    pub fn code(&self) -> Option<&str> {
        let structured = self.structured.as_ref()?;
        structured
            .get("code")
            .and_then(Value::as_str)
            .or_else(|| structured.pointer("/refusal/code").and_then(Value::as_str))
            .or_else(|| structured.pointer("/error/code").and_then(Value::as_str))
    }

    /// The first image block, as `(base64, mime)`.
    pub fn image(&self) -> Option<(&str, &str)> {
        self.content.iter().find_map(|c| {
            (c.get("type").and_then(Value::as_str) == Some("image")).then_some(())?;
            Some((
                c.get("data").and_then(Value::as_str)?,
                c.get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png"),
            ))
        })
    }
}
