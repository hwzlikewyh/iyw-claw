use super::{validate_specs, QuestionOption, QuestionSpec, MAX_HEADER_CHARS};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct QuestionInput {
    question: String,
    #[serde(default)]
    header: Option<String>,
    #[serde(default, rename = "multiSelect")]
    multi_select: bool,
    #[serde(default)]
    options: Vec<OptionInput>,
}

#[derive(Deserialize)]
struct OptionInput {
    label: String,
    #[serde(default)]
    description: String,
}

pub(super) fn parse(arguments: &Value) -> Result<Vec<QuestionSpec>, String> {
    let items = arguments
        .get("questions")
        .and_then(Value::as_array)
        .ok_or("ask_user_question requires a questions array")?;
    if items.is_empty() || items.len() > super::MAX_QUESTIONS {
        return Err(format!("expected 1..={} questions", super::MAX_QUESTIONS));
    }
    let specs = items
        .iter()
        .map(parse_item)
        .collect::<Result<Vec<_>, _>>()?;
    validate_specs(&specs)?;
    Ok(specs)
}

fn parse_item(value: &Value) -> Result<QuestionSpec, String> {
    let input: QuestionInput = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid question: {error}"))?;
    let question = input.question.trim().to_string();
    let header = input
        .header
        .map(|header| header.trim().to_string())
        .filter(|header| !header.is_empty())
        .unwrap_or_else(|| question.chars().take(MAX_HEADER_CHARS).collect());
    Ok(QuestionSpec {
        id: uuid::Uuid::new_v4().to_string(),
        question,
        header,
        multi_select: input.multi_select,
        options: input
            .options
            .into_iter()
            .map(|option| QuestionOption {
                label: option.label.trim().to_string(),
                description: option.description.trim().to_string(),
            })
            .collect(),
    })
}
