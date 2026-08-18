use serde_json::Value;
use uuid::Uuid;

use crate::acp::question::{
    QuestionOption, QuestionOutcome, QuestionSpec, MAX_QUESTION_TEXT_CHARS,
};

pub(super) fn result_card_input(specs: &[QuestionSpec]) -> Value {
    let questions: Vec<Value> = specs
        .iter()
        .map(|spec| {
            serde_json::json!({
                "question": spec.question,
                "header": spec.header,
                "multiSelect": spec.multi_select,
                "options": spec.options,
            })
        })
        .collect();
    serde_json::json!({ "questions": questions })
}

pub(super) fn result_card_output(outcome: &QuestionOutcome) -> Value {
    let answers: Vec<Value> = outcome
        .answers
        .iter()
        .map(|answer| {
            serde_json::json!({
                "header": answer.header,
                "question": answer.question,
                "multi_select": answer.multi_select,
                "selected": answer.selected,
            })
        })
        .collect();
    serde_json::json!({ "answers": answers, "declined": outcome.declined })
}

pub(super) fn approval_spec(message: &str) -> QuestionSpec {
    let question = if message.trim().is_empty() {
        "DeepSeek is asking for confirmation.".to_string()
    } else {
        limit(message)
    };
    QuestionSpec {
        id: format!("deepseek-approval-{}", Uuid::new_v4()),
        question,
        header: "Confirm".to_string(),
        multi_select: false,
        options: vec![
            QuestionOption {
                label: "Accept".to_string(),
                description: "Allow DeepSeek to continue.".to_string(),
            },
            QuestionOption {
                label: "Decline".to_string(),
                description: "Reject this request.".to_string(),
            },
        ],
    }
}

fn limit(value: &str) -> String {
    value.trim().chars().take(MAX_QUESTION_TEXT_CHARS).collect()
}
