use std::collections::{HashMap, HashSet};

use sacp::schema::{MultiSelectItems, StringPropertySchema};

use crate::acp::question::{QuestionOption, MAX_QUESTION_TEXT_CHARS};

/// 上游仍接受旧版 enumNames；ACP 使用等价的 oneOf，值和顺序保持不变。
pub(super) fn normalize_legacy_enums(raw: &mut serde_json::Value) -> Result<(), String> {
    let Some(properties) = raw
        .pointer_mut("/requestedSchema/properties")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return Ok(());
    };
    for property in properties.values_mut() {
        let Some(names) = property
            .get("enumNames")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let values = property
            .get("enum")
            .and_then(serde_json::Value::as_array)
            .ok_or("enumNames 缺少对应选项")?;
        if names.len() != values.len() || names.iter().any(|name| !name.is_string()) {
            return Err("enumNames 与枚举选项不匹配".into());
        }
        property["oneOf"] = serde_json::json!(names
            .iter()
            .zip(values)
            .map(|(name, value)| serde_json::json!({ "title": name, "const": value }))
            .collect::<Vec<_>>());
        if let Some(object) = property.as_object_mut() {
            object.remove("enumNames");
        }
    }
    Ok(())
}

/// ACP 的整数 schema 使用 i64 边界，上游允许小数边界；展示按 number 解码，
/// 原始 integer schema 另存于 QuestionInputSpec 并负责最终整数校验。
pub(super) fn normalize_integer_display(raw: &mut serde_json::Value) {
    let Some(properties) = raw
        .pointer_mut("/requestedSchema/properties")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    for property in properties
        .values_mut()
        .filter(|property| property["type"] == "integer")
    {
        property["type"] = serde_json::json!("number");
    }
}

pub(super) struct Choice {
    label: String,
    value: String,
}

pub(super) fn string_choices(schema: &StringPropertySchema) -> Vec<Choice> {
    if let Some(options) = &schema.one_of {
        return options
            .iter()
            .map(|option| choice(&option.title, &option.value))
            .collect();
    }
    schema
        .enum_values
        .as_ref()
        .map(|values| values.iter().map(|value| choice(value, value)).collect())
        .unwrap_or_default()
}

pub(super) fn array_choices(items: &MultiSelectItems) -> Vec<Choice> {
    match items {
        MultiSelectItems::Titled(items) => items
            .options
            .iter()
            .map(|option| choice(&option.title, &option.value))
            .collect(),
        MultiSelectItems::Untitled(items) => items
            .values
            .iter()
            .map(|value| choice(value, value))
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn normalize_choices(
    choices: Vec<Choice>,
) -> (Vec<QuestionOption>, HashMap<String, String>) {
    let mut options = Vec::new();
    let mut values = HashMap::new();
    let mut seen = HashSet::new();
    for (index, choice) in choices.into_iter().enumerate() {
        let base = if choice.label.trim().is_empty() {
            format!("选项 {}", index + 1)
        } else {
            limit(&choice.label)
        };
        let mut label = base.clone();
        let mut duplicate = 0;
        while seen.contains(&label) {
            duplicate += 1;
            let suffix = format!(" ({}-{duplicate})", index + 1);
            label = base
                .chars()
                .take(MAX_QUESTION_TEXT_CHARS - suffix.chars().count())
                .collect::<String>()
                + &suffix;
        }
        seen.insert(label.clone());
        values.insert(label.clone(), choice.value);
        options.push(QuestionOption {
            label,
            description: String::new(),
        });
    }
    (options, values)
}

pub(super) fn choice(label: &str, value: &str) -> Choice {
    Choice {
        label: if label.trim().is_empty() {
            value.to_string()
        } else {
            label.to_string()
        },
        value: value.to_string(),
    }
}

fn limit(value: &str) -> String {
    value.trim().chars().take(MAX_QUESTION_TEXT_CHARS).collect()
}
