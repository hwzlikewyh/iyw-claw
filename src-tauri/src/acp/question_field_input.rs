use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{QuestionAnswer, QuestionSpec, MAX_QUESTION_TEXT_CHARS};

#[path = "question_input_validation.rs"]
mod validation;

/// ACP 字段只在当前问答中使用；schema 不携带任意元数据或秘密默认值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionInputSpec {
    pub schema: Value,
    pub values: BTreeMap<String, String>,
    pub default_values: Vec<String>,
    pub allow_other: bool,
}

impl QuestionInputSpec {
    pub(crate) fn from_property(
        property: &Value,
        spec: &QuestionSpec,
        values: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut schema = property.clone();
        let object = schema.as_object_mut().ok_or("表单字段必须是对象")?;
        object.retain(|key, _| {
            matches!(
                key.as_str(),
                "type"
                    | "minLength"
                    | "maxLength"
                    | "minimum"
                    | "maximum"
                    | "pattern"
                    | "format"
                    | "enum"
                    | "oneOf"
                    | "items"
                    | "minItems"
                    | "maxItems"
            )
        });
        validation::validate_schema(&schema)?;
        let allow_other = spec.options.is_empty()
            || property.pointer("/_meta/codex/isOther") == Some(&json!(true));
        let mut input = Self {
            schema,
            values,
            default_values: Vec::new(),
            allow_other,
        };
        if !spec.secret {
            if let Some(value) = property.get("default").filter(|value| !value.is_null()) {
                // default 是提示值；无效提示不能让本来可填写的表单整体失效。
                let labels = input.default_labels(value, spec)?;
                if input.value(&labels).is_ok() {
                    input.default_values = labels;
                }
            }
        }
        Ok(input)
    }

    fn default_labels(&self, value: &Value, spec: &QuestionSpec) -> Result<Vec<String>, String> {
        let values = match value {
            Value::Array(values) => values.clone(),
            value => vec![value.clone()],
        };
        values
            .iter()
            .map(|value| {
                let text = match value {
                    Value::String(value) => value.clone(),
                    Value::Number(_) | Value::Bool(_) => value.to_string(),
                    _ => return Err("表单默认值类型无效".into()),
                };
                Ok(spec
                    .options
                    .iter()
                    .find(|option| self.values.get(&option.label) == Some(&text))
                    .map(|option| option.label.clone())
                    .unwrap_or(text))
            })
            .collect()
    }

    pub(crate) fn value(&self, labels: &[String]) -> Result<Value, String> {
        if labels
            .iter()
            .any(|value| value.chars().count() > MAX_QUESTION_TEXT_CHARS)
        {
            return Err(format!("输入不能超过 {MAX_QUESTION_TEXT_CHARS} 个字符"));
        }
        if !self.allow_other && labels.iter().any(|label| !self.values.contains_key(label)) {
            return Err("请从提供的选项中选择".into());
        }
        let values: Vec<&str> = labels
            .iter()
            .map(|label| self.values.get(label).unwrap_or(label).as_str())
            .collect();
        let value = typed_value(&self.schema, &values)?;
        validation::validate(&self.schema, &value, self.allow_other)?;
        Ok(value)
    }
}

fn typed_value(schema: &Value, values: &[&str]) -> Result<Value, String> {
    if schema["type"] == "array" {
        return Ok(json!(values));
    }
    if values.len() != 1 {
        return Err("请为此字段填写一个值".into());
    }
    let text = values[0];
    match schema["type"].as_str() {
        Some("string") => Ok(json!(text)),
        Some("boolean") => text
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|_| "请选择是或否".into()),
        Some("integer") => text
            .trim()
            .parse::<i64>()
            .map(|value| json!(value))
            .map_err(|_| "请输入整数".into()),
        Some("number") => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| json!(value))
            .ok_or("请输入有效数字".into()),
        _ => Err("不支持此表单字段类型".into()),
    }
}

pub(crate) fn validate_answers(
    questions: &[QuestionSpec],
    answer: &QuestionAnswer,
) -> Result<(), String> {
    if answer.declined {
        return Ok(());
    }
    for question in questions {
        let Some(input) = &question.input else {
            continue;
        };
        let mut matching = answer
            .answers
            .iter()
            .filter(|item| item.question_id == question.id);
        let labels = matching
            .next()
            .map(|item| item.labels.as_slice())
            .unwrap_or_default();
        if matching.next().is_some() {
            return Err("同一字段不能重复提交".into());
        }
        let maximum = if question.multi_select {
            question.options.len() + usize::from(input.allow_other)
        } else {
            1
        };
        if labels.len() > maximum {
            return Err("提交的选项数量超过此字段允许的范围".into());
        }
        if question.optional && labels.is_empty() {
            continue;
        }
        input
            .value(labels)
            .map_err(|error| format!("{}：{error}", question.header))?;
    }
    Ok(())
}
