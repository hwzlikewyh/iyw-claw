use std::collections::{HashMap, HashSet};

use sacp::schema::{MultiSelectItems, StringPropertySchema};

use crate::acp::question::{QuestionOption, MAX_OPTIONS, MAX_QUESTION_TEXT_CHARS};

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
    for choice in choices.into_iter().take(MAX_OPTIONS) {
        let label = limit(&choice.label);
        if label.is_empty() || !seen.insert(label.clone()) {
            continue;
        }
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
