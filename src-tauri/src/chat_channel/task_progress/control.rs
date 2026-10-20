use serde_json::Value;

use super::snapshot;
use crate::chat_channel::task_context;
use crate::chat_channel::types::{IncomingCommand, RichMessage};

enum Control {
    Status,
    Period(u64),
    Quiet,
    Milestones,
    Resume,
    Cancel,
}

pub(super) async fn handle(command: &IncomingCommand, prefix: &str) -> Option<RichMessage> {
    if command.callback_data.is_some() || !command.attachments.is_empty() {
        return None;
    }
    let text = command.command_text.trim();
    let text = text.strip_prefix(prefix).unwrap_or(text);
    let (text, selection) = text
        .split_once('#')
        .map(|(text, id)| (text.trim(), Some(id.trim())))
        .unwrap_or((text, None));
    let action = parse(text)?;
    let mut tasks = task_context::matching(&command.target, &command.sender_id);
    if let Some(id) = selection {
        tasks.retain(|task| id.len() >= 8 && task.id.starts_with(id));
    }
    let task = match tasks.as_slice() {
        [] => return Some(RichMessage::info("当前聊天没有正在跟踪的任务。")),
        [task] => task,
        _ => {
            let choices = tasks
                .iter()
                .map(|task| format!("{}：{}", &task.id[..8], task.origin.title))
                .collect::<Vec<_>>()
                .join("\n");
            return Some(RichMessage::info(format!(
                "当前有多个任务：\n{choices}\n请回复「进度 #任务编号」「停止汇报 #任务编号」或「取消任务 #任务编号」。"
            )));
        }
    };
    match action {
        Control::Status => {
            let value = match super::RUNTIME.get() {
                Some(runtime) => snapshot::view(runtime, task).await,
                None => Value::Null,
            };
            Some(RichMessage::info(snapshot::text(&value)))
        }
        Control::Cancel => Some(super::cancel(task).await),
        other => configure(task, other),
    }
}

fn configure(task: &task_context::TaskContext, action: Control) -> Option<RichMessage> {
    let (period, enabled, message) = match action {
        Control::Period(seconds) => (
            Some(seconds),
            Some(true),
            format!("已改为每 {seconds} 秒汇报，任务继续执行。"),
        ),
        Control::Quiet => (
            None,
            Some(false),
            "已停止周期汇报，任务继续执行，完成后仍会交付结果。".into(),
        ),
        Control::Milestones => (
            None,
            Some(false),
            "已切换为仅通知重要进展和最终结果。".into(),
        ),
        Control::Resume => (None, Some(true), "已恢复周期汇报。".into()),
        Control::Status | Control::Cancel => return None,
    };
    let result = task_context::configure(&task.id, period, enabled);
    tracing::info!(task_id = %task.id, channel_id = task.origin.route.channel_id,
                configured = result.is_ok(), "[channel-progress] reporting control");
    Some(RichMessage::info(result.map(|_| message).unwrap_or_else(
        |_| "汇报周期需要在 30 秒至 24 小时之间，任务仍继续执行。".into(),
    )))
}

fn parse(text: &str) -> Option<Control> {
    let text = text
        .trim()
        .trim_end_matches(['。', '！', '!', '？', '?'])
        .to_lowercase();
    match text.as_str() {
        "进度" | "查看进度" | "现在进度" | "现在做到哪了" | "做到哪了" | "progress"
        | "task status" => Some(Control::Status),
        "停止汇报"
        | "停止汇报，继续执行"
        | "停止汇报，继续做"
        | "停止汇报，任务继续"
        | "停止汇报,继续执行"
        | "停止汇报，继续"
        | "关闭进度汇报"
        | "stop reporting" => Some(Control::Quiet),
        "只告诉我重要进展" | "只汇报重要进展" | "仅重要进展" | "milestones only" => {
            Some(Control::Milestones)
        }
        "恢复汇报" | "开启进度汇报" | "resume reporting" => Some(Control::Resume),
        "取消这个任务" | "取消任务" | "停止任务" | "cancel" | "cancel task" => {
            Some(Control::Cancel)
        }
        _ => parse_period(&text),
    }
}

fn parse_period(text: &str) -> Option<Control> {
    let text = text
        .strip_prefix("改成")
        .or_else(|| text.strip_prefix("改为"))
        .or_else(|| text.strip_prefix("进度 "))
        .unwrap_or(text)
        .trim();
    let value = text.strip_prefix("每")?;
    let value = value
        .strip_suffix("汇报一次")
        .or_else(|| value.strip_suffix("汇报"))
        .or_else(|| value.strip_suffix("报告一次"))?;
    let (number, multiplier) = if let Some(number) = value.strip_suffix("分钟") {
        (number, 60)
    } else if let Some(number) = value.strip_suffix("小时") {
        (number, 3600)
    } else {
        (value.strip_suffix('秒')?, 1)
    };
    let number = match number.trim() {
        "一" => 1,
        "两" | "二" => 2,
        "三" => 3,
        "五" => 5,
        "十" => 10,
        "十五" => 15,
        "三十" => 30,
        other => other.parse::<u64>().ok()?,
    };
    Some(Control::Period(number.saturating_mul(multiplier)))
}

pub(crate) fn initial_interval(text: &str) -> Option<u64> {
    let start = text.rfind('每')?;
    let clause = text[..start]
        .rsplit(['\n', '，', ',', '。', '；', ';'])
        .next()
        .unwrap_or_default();
    if ["不要", "不用", "不需要", "别再", "停止", "关闭"]
        .iter()
        .any(|word| clause.contains(word))
    {
        return None;
    }
    let suffix = &text[start..];
    for end in ["汇报一次", "汇报", "报告一次"] {
        if let Some(index) = suffix.find(end) {
            if let Some(Control::Period(seconds)) = parse_period(&suffix[..index + end.len()]) {
                return Some(seconds.clamp(
                    task_context::MIN_REPORT_SECONDS,
                    task_context::MAX_REPORT_SECONDS,
                ));
            }
        }
    }
    None
}
