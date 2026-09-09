use rmcp::ErrorData;
use serde_json::{json, Value};

const MAX_DESCRIPTION_CHARS: usize = 120;

pub(super) fn schema() -> Value {
    json!({
        "type": "string", "minLength": 1, "maxLength": MAX_DESCRIPTION_CHARS,
        "description": "A short user-facing description of the current action, in the user's language, for example 查询产品列表 or 上传设计文件. Describe the action and object; omit tokens, personal data, URLs, and implementation details. This is displayed as progress and is never sent to the website API."
    })
}

pub(super) fn validate(description: Option<&str>) -> Result<(), ErrorData> {
    let Some(description) = description else {
        // 兼容已保存的旧工具调用，新调用按 schema 提供描述。
        return Ok(());
    };
    if description.trim().is_empty()
        || description.chars().count() > MAX_DESCRIPTION_CHARS
        || description.chars().any(char::is_control)
    {
        return Err(ErrorData::invalid_params(
            "description must contain 1 to 120 characters without control characters",
            Some(json!({"code": "invalid_request", "execution_status": "not_started"})),
        ));
    }
    Ok(())
}
