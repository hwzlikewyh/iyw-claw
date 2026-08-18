use std::collections::{BTreeMap, HashMap};

use sacp::schema::{
    CreateElicitationRequest, CreateElicitationResponse, ElicitationAcceptAction,
    ElicitationAction, ElicitationContentValue, ElicitationMode, ElicitationPropertySchema,
    ElicitationScope, SessionId,
};
use serde_json::Value;

use crate::acp::deepseek_elicitation_card::{approval_spec, result_card_input, result_card_output};
use crate::acp::deepseek_elicitation_choices::{
    array_choices, choice, normalize_choices, string_choices, Choice,
};
use crate::acp::question::{
    QuestionOutcome, QuestionSpec, MAX_HEADER_CHARS, MAX_QUESTIONS, MAX_QUESTION_TEXT_CHARS,
};

#[derive(Clone, Copy)]
enum FieldKind {
    Text,
    MultiSelect,
    Boolean,
    Number,
    Integer,
}

struct FieldPlan {
    id: String,
    kind: FieldKind,
    value_by_label: HashMap<String, String>,
}

pub(super) struct FormPlan {
    specs: Vec<QuestionSpec>,
    fields: Vec<FieldPlan>,
    approval: bool,
    tool_call_id: Option<String>,
}

impl FormPlan {
    pub(super) fn specs(&self) -> &[QuestionSpec] {
        &self.specs
    }

    pub(super) fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub(super) fn is_approval(&self) -> bool {
        self.approval
    }

    pub(super) fn tool_call_id(&self) -> Option<&str> {
        self.tool_call_id.as_deref()
    }

    /// Build the input shape understood by the existing ask-question result
    /// card. DeepSeek resolves elicitation out-of-band, so this synthetic
    /// event keeps the answered request visible in the live transcript.
    pub(super) fn result_card_input(&self) -> Value {
        result_card_input(&self.specs)
    }

    pub(super) fn result_card_output(&self, outcome: &QuestionOutcome) -> Value {
        result_card_output(outcome)
    }

    pub(super) fn response(&self, outcome: &QuestionOutcome) -> CreateElicitationResponse {
        if self.approval {
            let accepted = !outcome.declined
                && outcome.answers.len() == 1
                && outcome.answers[0]
                    .selected
                    .first()
                    .is_some_and(|value| value == "Accept");
            return if accepted {
                CreateElicitationResponse::new(ElicitationAction::Accept(
                    ElicitationAcceptAction::new(),
                ))
            } else {
                decline_response()
            };
        }
        if outcome.declined || outcome.answers.len() != self.fields.len() {
            return decline_response();
        }
        let mut content = BTreeMap::new();
        for (field, answer) in self.fields.iter().zip(&outcome.answers) {
            let mapped = answer
                .selected
                .iter()
                .map(|label| {
                    field
                        .value_by_label
                        .get(label)
                        .cloned()
                        .unwrap_or_else(|| label.clone())
                })
                .collect();
            let Some(value) = typed_value(field.kind, mapped) else {
                return decline_response();
            };
            content.insert(field.id.clone(), value);
        }
        CreateElicitationResponse::new(ElicitationAction::Accept(
            ElicitationAcceptAction::new().content(content),
        ))
    }
}

pub(super) fn parse_request(raw: Value) -> Result<(SessionId, FormPlan), String> {
    let request: CreateElicitationRequest = serde_json::from_value(raw)
        .map_err(|error| format!("invalid elicitation request: {error}"))?;
    let ElicitationMode::Form(form) = request.mode else {
        return Err("only form elicitation is supported".to_string());
    };
    let ElicitationScope::Session(scope) = &form.scope else {
        return Err("elicitation is not tied to a session".to_string());
    };
    let tool_call_id = scope.tool_call_id.as_ref().map(|value| value.0.to_string());
    let mut plan = parse_form(&form.requested_schema.properties, &request.message);
    if plan.specs.is_empty() {
        plan = approval_plan(&request.message, tool_call_id.clone());
    } else {
        plan.tool_call_id = tool_call_id;
    }
    Ok((scope.session_id.clone(), plan))
}

pub(super) fn decline_response() -> CreateElicitationResponse {
    CreateElicitationResponse::new(ElicitationAction::Decline)
}

fn parse_form(properties: &BTreeMap<String, ElicitationPropertySchema>, message: &str) -> FormPlan {
    let mut specs = Vec::new();
    let mut fields = Vec::new();
    for (id, property) in properties.iter().take(MAX_QUESTIONS) {
        let (title, description, kind, multi_select, choices) = property_parts(property);
        let question = description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| title.as_deref().filter(|value| !value.trim().is_empty()))
            .or_else(|| (!message.trim().is_empty()).then_some(message))
            .unwrap_or(id);
        let header_source = title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(question);
        let (options, value_by_label) = normalize_choices(choices);
        specs.push(QuestionSpec {
            id: id.clone(),
            question: limit(question, MAX_QUESTION_TEXT_CHARS),
            header: limit(header_source, MAX_HEADER_CHARS),
            multi_select,
            options,
        });
        fields.push(FieldPlan {
            id: id.clone(),
            kind,
            value_by_label,
        });
    }
    if properties.len() > MAX_QUESTIONS {
        tracing::warn!(
            agent = "deepseek",
            field_count = properties.len(),
            max_fields = MAX_QUESTIONS,
            "[ACP] elicitation fields were truncated"
        );
    }
    FormPlan {
        specs,
        fields,
        approval: false,
        tool_call_id: None,
    }
}

fn approval_plan(message: &str, tool_call_id: Option<String>) -> FormPlan {
    FormPlan {
        specs: vec![approval_spec(message)],
        fields: Vec::new(),
        approval: true,
        tool_call_id,
    }
}

fn property_parts(
    property: &ElicitationPropertySchema,
) -> (Option<String>, Option<String>, FieldKind, bool, Vec<Choice>) {
    match property {
        ElicitationPropertySchema::String(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Text,
            false,
            string_choices(schema),
        ),
        ElicitationPropertySchema::Array(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::MultiSelect,
            true,
            array_choices(&schema.items),
        ),
        ElicitationPropertySchema::Boolean(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Boolean,
            false,
            vec![choice("Yes", "true"), choice("No", "false")],
        ),
        ElicitationPropertySchema::Number(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Number,
            false,
            Vec::new(),
        ),
        ElicitationPropertySchema::Integer(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Integer,
            false,
            Vec::new(),
        ),
        _ => (None, None, FieldKind::Text, false, Vec::new()),
    }
}

fn typed_value(kind: FieldKind, values: Vec<String>) -> Option<ElicitationContentValue> {
    match kind {
        FieldKind::Text => values
            .into_iter()
            .next()
            .map(ElicitationContentValue::String),
        FieldKind::MultiSelect => Some(ElicitationContentValue::StringArray(values)),
        FieldKind::Boolean => parse_bool(values.first()?).map(ElicitationContentValue::Boolean),
        FieldKind::Number => values
            .first()?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(ElicitationContentValue::Number),
        FieldKind::Integer => values
            .first()?
            .trim()
            .parse::<i64>()
            .ok()
            .map(ElicitationContentValue::Integer),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" => Some(true),
        "false" | "no" | "n" => Some(false),
        _ => None,
    }
}

fn limit(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}
