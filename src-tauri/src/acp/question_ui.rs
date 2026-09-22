use super::{QuestionAnswer, QuestionSpec, MAX_QUESTION_TEXT_CHARS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const MAX_REPEAT_ROWS: usize = 20;
const MAX_REPEAT_COLUMNS: usize = 6;
const MAX_FIELD_ID_CHARS: usize = 128;
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionUi {
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub control: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub when: Option<QuestionCondition>,
    #[serde(default)]
    pub columns: Vec<QuestionColumn>,
    #[serde(default)]
    pub max_rows: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionCondition {
    pub question_id: String,
    pub equals: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionColumn {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder: Option<String>,
}

pub(super) fn validate_ui(questions: &[QuestionSpec]) -> Result<(), String> {
    for (index, question) in questions.iter().enumerate() {
        if question.id.chars().count() > MAX_FIELD_ID_CHARS {
            return Err("Question id is too long".into());
        }
        let Some(ui) = &question.ui else { continue };
        validate_control(question, ui)?;
        validate_columns(ui)?;
        validate_date_reference(question, &questions[..index])?;
        if ui
            .layout
            .as_deref()
            .is_some_and(|value| !["form", "steps"].contains(&value))
        {
            return Err("Question layout must be form or steps".into());
        }
        for text in [&ui.group, &ui.placeholder].into_iter().flatten() {
            if text.chars().count() > MAX_QUESTION_TEXT_CHARS {
                return Err("Question UI text is too long".into());
            }
        }
        if let Some(condition) = &ui.when {
            let parent = questions[..index]
                .iter()
                .find(|parent| parent.id == condition.question_id)
                .ok_or("Conditions must reference an earlier question id")?;
            if parent.secret
                || parent.ui.as_ref().and_then(|ui| ui.control.as_deref()) == Some("repeat")
            {
                return Err("Conditions cannot reference secret or repeated fields".into());
            }
            if condition.equals.chars().count() > MAX_QUESTION_TEXT_CHARS {
                return Err("Condition value is too long".into());
            }
        }
    }
    Ok(())
}

fn validate_date_reference(
    question: &QuestionSpec,
    previous: &[QuestionSpec],
) -> Result<(), String> {
    let Some(id) = question.ui.as_ref().and_then(|ui| ui.not_before.as_ref()) else {
        return Ok(());
    };
    let parent = previous
        .iter()
        .find(|parent| parent.id == *id)
        .ok_or("Date constraints must reference an earlier field")?;
    if ![question, parent].iter().all(|field| {
        field
            .input
            .as_ref()
            .is_some_and(|input| input.schema["format"] == "date")
    }) {
        return Err("not_before requires date fields".into());
    }
    Ok(())
}

fn validate_control(question: &QuestionSpec, ui: &QuestionUi) -> Result<(), String> {
    let Some(control) = ui.control.as_deref() else {
        return Ok(());
    };
    if ![
        "text", "textarea", "select", "combobox", "radio", "checkbox", "number", "date", "switch",
        "password", "repeat",
    ]
    .contains(&control)
    {
        return Err("Unsupported question control".into());
    }
    let kind = question
        .input
        .as_ref()
        .and_then(|input| input.schema["type"].as_str());
    let valid = match control {
        "select" | "combobox" | "radio" => !question.multi_select && !question.options.is_empty(),
        "checkbox" => question.multi_select && !question.options.is_empty(),
        "switch" => kind == Some("boolean"),
        "number" => matches!(kind, Some("number" | "integer")),
        "date" => {
            kind == Some("string")
                && question
                    .input
                    .as_ref()
                    .is_some_and(|input| input.schema["format"] == "date")
        }
        "password" => question.secret && question.options.is_empty(),
        "repeat" => {
            kind == Some("string")
                && question.options.is_empty()
                && !question.secret
                && !question.multi_select
        }
        _ => question.options.is_empty() && matches!(kind, None | Some("string")),
    };
    if !valid {
        return Err("Question control does not match its input or options".into());
    }
    if question.secret && control != "password" {
        return Err("Secret fields require a password control".into());
    }
    Ok(())
}

fn validate_columns(ui: &QuestionUi) -> Result<(), String> {
    if ui.control.as_deref() != Some("repeat") {
        return if ui.columns.is_empty() && ui.max_rows.is_none() {
            Ok(())
        } else {
            Err("Columns are only supported by repeat controls".into())
        };
    }
    if ui.columns.is_empty()
        || ui.columns.len() > MAX_REPEAT_COLUMNS
        || !(1..=MAX_REPEAT_ROWS).contains(&ui.max_rows.unwrap_or(MAX_REPEAT_ROWS))
    {
        return Err("Repeated fields require 1..6 columns and 1..20 rows".into());
    }
    let mut ids = HashSet::new();
    for column in &ui.columns {
        if column.id.trim().is_empty()
            || column.id.chars().count() > MAX_FIELD_ID_CHARS
            || !ids.insert(&column.id)
            || column.label.trim().is_empty()
            || column.label.chars().count() > MAX_QUESTION_TEXT_CHARS
            || column
                .placeholder
                .as_ref()
                .is_some_and(|text| text.chars().count() > MAX_QUESTION_TEXT_CHARS)
        {
            return Err("Invalid repeated field column".into());
        }
    }
    Ok(())
}

pub fn active_questions(questions: &[QuestionSpec], answer: &QuestionAnswer) -> Vec<QuestionSpec> {
    let mut active = Vec::new();
    let mut selected = std::collections::HashMap::new();
    for question in questions {
        let visible = question
            .ui
            .as_ref()
            .and_then(|ui| ui.when.as_ref())
            .is_none_or(|condition| {
                selected
                    .get(&condition.question_id)
                    .is_some_and(|labels: &Vec<String>| labels.contains(&condition.equals))
            });
        if visible {
            let outcome = super::build_outcome(std::slice::from_ref(question), answer);
            let labels = outcome.answers.first().map(|item| item.selected.clone());
            selected.insert(question.id.clone(), labels.unwrap_or_default());
            active.push(question.clone());
        }
    }
    active
}

pub fn validate_answer_ids(answer: &QuestionAnswer) -> Result<(), String> {
    let mut ids = HashSet::new();
    if answer.answers.len() > super::MAX_QUESTIONS {
        return Err("Too many answers".into());
    }
    for item in &answer.answers {
        if !ids.insert(&item.question_id) {
            return Err("Duplicate question answer".into());
        }
    }
    Ok(())
}

pub(super) fn validate_date_order(
    questions: &[QuestionSpec],
    answer: &QuestionAnswer,
) -> Result<(), String> {
    for question in questions {
        let Some(parent) = question.ui.as_ref().and_then(|ui| ui.not_before.as_ref()) else {
            continue;
        };
        if !questions.iter().any(|field| field.id == *parent) {
            continue;
        }
        let value = answer
            .answers
            .iter()
            .find(|item| item.question_id == question.id)
            .and_then(|item| item.labels.first());
        let minimum = answer
            .answers
            .iter()
            .find(|item| item.question_id == *parent)
            .and_then(|item| item.labels.first());
        if value.zip(minimum).is_some_and(|(value, minimum)| {
            !value.is_empty() && !minimum.is_empty() && value < minimum
        }) {
            return Err(format!(
                "{}: date is before the required start date",
                question.header
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_repeated(question: &QuestionSpec, labels: &[String]) -> Result<(), String> {
    let Some(ui) = question
        .ui
        .as_ref()
        .filter(|ui| ui.control.as_deref() == Some("repeat"))
    else {
        return Ok(());
    };
    if labels.is_empty() && question.optional {
        return Ok(());
    }
    if labels.len() != 1 {
        return Err("Repeated fields require one JSON array answer".into());
    }
    let rows: Vec<Value> =
        serde_json::from_str(&labels[0]).map_err(|_| "Invalid repeated field rows")?;
    if rows.len() > ui.max_rows.unwrap_or(MAX_REPEAT_ROWS)
        || (!question.optional && rows.is_empty())
    {
        return Err("Invalid repeated field row count".into());
    }
    for row in rows {
        let object = row
            .as_object()
            .ok_or("Repeated field rows must be objects")?;
        if object
            .keys()
            .any(|key| !ui.columns.iter().any(|column| column.id == *key))
        {
            return Err("Unknown repeated field column".into());
        }
        for column in &ui.columns {
            let value = object.get(&column.id);
            if value.is_some_and(|value| !value.is_string()) {
                return Err("Repeated field values must be strings".into());
            }
            if column.required
                && value
                    .and_then(Value::as_str)
                    .is_none_or(|text| text.trim().is_empty())
            {
                return Err(format!("{} is required", column.label));
            }
        }
    }
    Ok(())
}
