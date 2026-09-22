use super::{
    validate_specs, QuestionInputSpec, QuestionOption, QuestionSpec, QuestionUi, MAX_HEADER_CHARS,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct QuestionInput {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    secret: bool,
    #[serde(default)]
    input: Option<Value>,
    #[serde(default)]
    ui: Option<QuestionUi>,
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
    let mut specs = items
        .iter()
        .map(parse_item)
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(layout) = arguments.get("layout") {
        let layout = layout
            .as_str()
            .filter(|value| ["form", "steps"].contains(value))
            .ok_or("layout must be form or steps")?;
        for spec in &mut specs {
            spec.ui.get_or_insert_with(QuestionUi::default).layout = Some(layout.into());
        }
    }
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
    let mut spec = QuestionSpec {
        ui: input.ui,
        input: None,
        secret: input.secret,
        optional: input.optional,
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
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
    };
    apply_input(&mut spec, input.input)?;
    Ok(spec)
}

fn apply_input(spec: &mut QuestionSpec, property: Option<Value>) -> Result<(), String> {
    let control = spec.ui.as_ref().and_then(|ui| ui.control.as_deref());
    spec.secret |= control == Some("password");
    let kind = match control {
        Some("switch") => "boolean",
        Some("checkbox") => "array",
        Some("number") => "number",
        _ if spec.multi_select => "array",
        _ => "string",
    };
    if property.is_none() && spec.ui.is_none() && !spec.secret && !spec.optional {
        return Ok(());
    }
    let mut property = property.unwrap_or_else(|| serde_json::json!({"type": kind}));
    let object = property
        .as_object_mut()
        .ok_or("input must be a JSON Schema property")?;
    object
        .entry("type")
        .or_insert_with(|| Value::String(kind.into()));
    if control == Some("date") {
        object.insert("format".into(), Value::String("date".into()));
    }
    if object.get("type") == Some(&Value::String("string".into())) && !spec.optional {
        object.entry("minLength").or_insert_with(|| Value::from(1));
    }
    if object.get("type") == Some(&Value::String("array".into())) && !spec.optional {
        object.entry("minItems").or_insert_with(|| Value::from(1));
    }
    if !spec.options.is_empty() && !matches!(control, Some("select" | "combobox" | "switch")) {
        object.insert("_meta".into(), serde_json::json!({"codex/isOther": true}));
    }
    normalize_field_options(spec, &property);
    let values = spec
        .options
        .iter()
        .map(|option| (option.label.clone(), option.label.clone()))
        .collect();
    spec.input = Some(QuestionInputSpec::from_property(&property, spec, values)?);
    Ok(())
}

fn normalize_field_options(spec: &mut QuestionSpec, property: &Value) {
    if property["type"] == "array" {
        spec.multi_select = true;
    }
    if property["type"] == "boolean" && spec.options.is_empty() {
        spec.options = ["true", "false"]
            .into_iter()
            .map(|label| QuestionOption {
                label: label.into(),
                description: String::new(),
            })
            .collect();
    }
}
