use super::{button, Action};
use crate::acp::manager::ConnectionManager;
use crate::acp::question::{QuestionAnswer, QuestionAnswerItem, QuestionSpec};
use crate::chat_channel::task_context::{self, TaskContext};
use crate::chat_channel::types::*;
use serde_json::Value;

pub(super) fn card(
    task: &TaskContext,
    id: &str,
    questions: &[QuestionSpec],
) -> Option<InteractiveMessage> {
    let mut body = String::new();
    let mut buttons = vec![];
    if questions.iter().any(|question| question.secret) {
        body.push_str("包含敏感字段，请在 iyw-claw 原会话中填写，勿在聊天渠道发送凭据。");
    } else {
        for question in questions {
            body.push_str(&format!("{}：{}\n", question.id, question.question));
            for (index, option) in question.options.iter().enumerate() {
                body.push_str(&format!("{}. {}\n", index + 1, option.label));
            }
        }
        if let [question] = questions {
            if !question.multi_select && question.input.is_none() {
                buttons = question
                    .options
                    .iter()
                    .take(6)
                    .map(|option| {
                        button(
                            task,
                            &option.label,
                            Action::Answer(
                                id.to_string(),
                                question.id.clone(),
                                option.label.clone(),
                            ),
                        )
                    })
                    .collect();
            }
            body.push_str("直接回复答案；选项可回复编号，多选用逗号分隔。取消回答请回复 /skip。\n");
        } else {
            body.push_str("请用 JSON 一次回答，例如 {\"字段ID\":\"答案\"}；多选值用数组。取消请回复 /skip。\n");
        }
    }
    Some(InteractiveMessage {
        base: RichMessage::info(body).with_title("需要你的回答"),
        buttons,
        callback_context: serde_json::json!({"question_id":id}),
    })
}

pub(super) async fn answer_text(
    command: &IncomingCommand,
    manager: &ConnectionManager,
) -> Option<RichMessage> {
    let text = command.command_text.trim();
    if !accepts_text(command, text) {
        return None;
    }
    let tasks = task_context::matching(&command.target, &command.sender_id)
        .into_iter()
        .filter(|task| task.tools_active)
        .collect::<Vec<_>>();
    let [task] = tasks.as_slice() else {
        return None;
    };
    let state = manager.get_state(&task.connection_id).await?;
    let pending = {
        let state = state.read().await;
        if state.turn_generation != task.turn_generation || !state.turn_in_flight {
            return None;
        }
        state.pending_question.clone()?
    };
    if chrono::Utc::now()
        .signed_duration_since(pending.created_at)
        .num_seconds()
        >= super::INTERACTION_TTL.as_secs() as i64
    {
        return Some(RichMessage::info(
            "此问题的渠道回答已过期，请在原会话处理。",
        ));
    }
    if pending.questions.iter().any(|question| question.secret) {
        return Some(RichMessage::info("请在 iyw-claw 原会话填写敏感字段。"));
    }
    let answer = match parse_answer(&pending.questions, text) {
        Ok(answer) => answer,
        Err(error) => return Some(RichMessage::error(error)),
    };
    Some(
        match manager
            .answer_question(&task.connection_id, &pending.question_id, answer)
            .await
        {
            Ok(()) => RichMessage::info("已提交回答，继续处理任务。"),
            Err(error) => RichMessage::error(format!("回答未通过校验，请重新填写：{error}")),
        },
    )
}

fn parse_answer(questions: &[QuestionSpec], text: &str) -> Result<QuestionAnswer, String> {
    if text == "/skip" {
        return Ok(QuestionAnswer {
            answers: vec![],
            declined: true,
        });
    }
    if let [question] = questions {
        let labels = if question.multi_select {
            text.split([',', '，'])
                .map(str::trim)
                .map(str::to_string)
                .collect()
        } else {
            vec![text.to_string()]
        };
        let labels = labels
            .into_iter()
            .map(|label| {
                label
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| question.options.get(index))
                    .map(|option| option.label.clone())
                    .unwrap_or(label)
            })
            .collect();
        return Ok(QuestionAnswer {
            answers: vec![QuestionAnswerItem {
                question_id: question.id.clone(),
                labels,
            }],
            declined: false,
        });
    }
    parse_fields(questions, text)
}

fn parse_fields(questions: &[QuestionSpec], text: &str) -> Result<QuestionAnswer, String> {
    let data: Value = serde_json::from_str(text).map_err(|_| "请按字段 ID 用 JSON 回答。")?;
    let fields = data.as_object().ok_or("请使用 JSON 对象回答问题。")?;
    if fields
        .keys()
        .any(|id| !questions.iter().any(|question| &question.id == id))
    {
        return Err("回答包含未知字段 ID，请使用最新问题中的字段。".into());
    }
    let answers = questions
        .iter()
        .filter_map(|question| {
            let value = data.get(&question.id)?;
            let labels = match value {
                Value::Array(items) => items.iter().map(value_text).collect(),
                value => vec![value_text(value)],
            };
            Some(QuestionAnswerItem {
                question_id: question.id.clone(),
                labels,
            })
        })
        .collect();
    Ok(QuestionAnswer {
        answers,
        declined: false,
    })
}

fn value_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

pub(super) fn progress_text(value: Value) -> String {
    format!(
        "当前状态：{}\n已运行 {} 秒\n{}",
        value["phase"].as_str().unwrap_or("unknown"),
        value["elapsed_seconds"].as_u64().unwrap_or(0),
        value["summary"].as_str().unwrap_or("暂无新的阶段说明")
    )
}

fn accepts_text(command: &IncomingCommand, text: &str) -> bool {
    !(command.callback_data.is_some()
        || !command.attachments.is_empty()
        || (text.starts_with('/') && text != "/skip")
        || matches!(text, "取消这个任务" | "取消任务" | "停止任务"))
}
