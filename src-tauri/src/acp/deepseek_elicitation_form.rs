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
    QuestionOutcome, QuestionSpec, MAX_HEADER_CHARS, MAX_QUESTION_TEXT_CHARS,
};

struct FieldPlan {
    optional: bool,
    id: String,
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
        let mut visible = outcome.clone();
        for answer in &mut visible.answers {
            if self.specs.iter().any(|spec| {
                spec.secret && spec.question == answer.question && spec.header == answer.header
            }) {
                answer.selected = vec!["[已隐藏秘密输入]".to_string()];
            }
        }
        result_card_output(&visible)
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
        for ((field, answer), spec) in self.fields.iter().zip(&outcome.answers).zip(&self.specs) {
            if field.optional && answer.selected.is_empty() {
                continue;
            }
            let value = spec
                .input
                .as_ref()
                .and_then(|input| input.value(&answer.selected).ok());
            let Some(value) = value
                .and_then(|value| serde_json::from_value::<ElicitationContentValue>(value).ok())
            else {
                return decline_response();
            };
            content.insert(field.id.clone(), value);
        }
        CreateElicitationResponse::new(ElicitationAction::Accept(
            ElicitationAcceptAction::new().content(content),
        ))
    }
}

pub(super) fn parse_request(mut raw: Value) -> Result<(SessionId, FormPlan), String> {
    crate::acp::deepseek_elicitation_choices::normalize_legacy_enums(&mut raw)?;
    let properties_raw = raw
        .pointer("/requestedSchema/properties")
        .cloned()
        .unwrap_or_default();
    let required_fields = raw
        .pointer("/requestedSchema/required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    const MAX_FORM_FIELDS: usize = 128;
    if properties_raw
        .as_object()
        .is_some_and(|properties| properties.len() > MAX_FORM_FIELDS)
    {
        return Err("elicitation form exceeds the supported field limit".into());
    }
    crate::acp::deepseek_elicitation_choices::normalize_integer_display(&mut raw);
    let request: CreateElicitationRequest =
        serde_json::from_value(raw).map_err(|_| "invalid elicitation request shape".to_string())?;
    let ElicitationMode::Form(form) = request.mode else {
        return Err("only form elicitation is supported".to_string());
    };
    let ElicitationScope::Session(scope) = &form.scope else {
        return Err("elicitation is not tied to a session".to_string());
    };
    let tool_call_id = scope.tool_call_id.as_ref().map(|value| value.0.to_string());
    let mut plan = parse_form(&form.requested_schema.properties, &request.message);
    apply_field_schemas(&mut plan, &properties_raw, &required_fields)?;
    if plan.specs.is_empty() {
        plan = approval_plan(&request.message, tool_call_id.clone());
    } else {
        plan.tool_call_id = tool_call_id;
    }
    Ok((scope.session_id.clone(), plan))
}

fn apply_field_schemas(
    plan: &mut FormPlan,
    properties_raw: &Value,
    required_fields: &[Value],
) -> Result<(), String> {
    if required_fields.iter().any(|field| {
        field
            .as_str()
            .is_none_or(|id| properties_raw.get(id).is_none())
    }) {
        return Err("表单必填列表包含未定义的字段".into());
    }
    for (spec, field) in plan.specs.iter_mut().zip(&mut plan.fields) {
        let property = &properties_raw[&spec.id];
        spec.secret = property
            .pointer("/_meta/codex/isSecret")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        spec.optional = !required_fields
            .iter()
            .any(|field| field.as_str() == Some(spec.id.as_str()));
        field.optional = spec.optional;
        if let Some(choices) = property
            .get("oneOf")
            .or_else(|| property.pointer("/items/anyOf"))
            .and_then(Value::as_array)
        {
            for (option, choice) in spec.options.iter_mut().zip(choices) {
                option.description = choice["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
            }
        }
        spec.input = Some(crate::acp::question::QuestionInputSpec::from_property(
            property,
            spec,
            field.value_by_label.clone().into_iter().collect(),
        )?);
    }
    Ok(())
}

pub(super) fn decline_response() -> CreateElicitationResponse {
    CreateElicitationResponse::new(ElicitationAction::Decline)
}

fn parse_form(properties: &BTreeMap<String, ElicitationPropertySchema>, message: &str) -> FormPlan {
    let mut specs = Vec::new();
    let mut fields = Vec::new();
    for (id, property) in properties {
        let (title, description, multi_select, choices) = property_parts(property);
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
            input: None,
            secret: false,
            optional: false,
            id: id.clone(),
            question: limit(question, MAX_QUESTION_TEXT_CHARS),
            header: limit(header_source, MAX_HEADER_CHARS),
            multi_select,
            options,
        });
        fields.push(FieldPlan {
            optional: false,
            id: id.clone(),
            value_by_label,
        });
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
) -> (Option<String>, Option<String>, bool, Vec<Choice>) {
    match property {
        ElicitationPropertySchema::String(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            false,
            string_choices(schema),
        ),
        ElicitationPropertySchema::Array(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            true,
            array_choices(&schema.items),
        ),
        ElicitationPropertySchema::Boolean(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            false,
            vec![choice("Yes", "true"), choice("No", "false")],
        ),
        ElicitationPropertySchema::Number(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            false,
            Vec::new(),
        ),
        ElicitationPropertySchema::Integer(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            false,
            Vec::new(),
        ),
        _ => (None, None, false, Vec::new()),
    }
}

fn limit(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}
