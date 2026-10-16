use sea_orm::DatabaseConnection;

use crate::chat_channel::{i18n::Lang, types::RichMessage};
use crate::db::{entities::automation_run, service::app_metadata_service};
use crate::models::AutomationRunStatus;

const SUMMARY_CHAR_LIMIT: usize = 1200;

pub async fn format(
    db: &DatabaseConnection,
    run: &automation_run::Model,
    name: &str,
) -> RichMessage {
    let lang = app_metadata_service::get_value(db, "chat_message_language")
        .await
        .ok()
        .flatten()
        .map(|value| Lang::from_str_lossy(&value))
        .unwrap_or_default();
    let chinese = matches!(lang, Lang::ZhCn | Lang::ZhTw);
    let status = match (&run.status, chinese) {
        (AutomationRunStatus::Succeeded, true) => "执行成功",
        (AutomationRunStatus::Cancelled, true) => "已取消",
        (_, true) => "执行失败",
        (AutomationRunStatus::Succeeded, false) => "Succeeded",
        (AutomationRunStatus::Cancelled, false) => "Cancelled",
        (_, false) => "Failed",
    };
    let hint = if chinese {
        "可在任务执行记录中查看详情。"
    } else {
        "View this run in Scheduled tasks."
    };
    let summary = run
        .summary
        .as_deref()
        .or(run.error.as_deref())
        .unwrap_or(hint);
    let source = match (run.trigger.as_str(), chinese) {
        ("retry", true) => "补跑",
        ("manual", true) => "手动",
        (_, true) => "定时触发",
        ("retry", false) => "Retry",
        ("manual", false) => "Manual",
        (_, false) => "Scheduled",
    };
    let location = if chinese {
        format!("定时任务 → {name} → 执行 #{}", run.id)
    } else {
        format!("Scheduled tasks → {name} → Run #{}", run.id)
    };
    RichMessage::info(summary.chars().take(SUMMARY_CHAR_LIMIT).collect::<String>())
        .with_title(format!("{name} · {status}"))
        .with_field(if chinese { "执行编号" } else { "Run" }, run.id.to_string())
        .with_field(
            if chinese {
                "查看结果"
            } else {
                "View result"
            },
            location,
        )
        .with_field(if chinese { "执行来源" } else { "Trigger" }, source)
}
