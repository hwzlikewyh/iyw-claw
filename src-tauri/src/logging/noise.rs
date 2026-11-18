//! 默认降噪：高频且无诊断价值的第三方 target 只保留更严重的记录。

use super::LogLevel;

/// 实测占单日日志体积约一半的 target 及其默认上限级别。
const QUIET_TARGETS: &[(&str, LogLevel)] = &[
    // 每次模型请求都会输出两条「重新加载凭据」信息。
    ("codex_login::auth::manager", LogLevel::Warn),
    // 每次模型请求输出数 KB 的反馈标签快照，主进程没有消费方。
    ("feedback_tags", LogLevel::Warn),
    // 每次创建 HTTP 客户端都会重复同一条根证书说明。
    ("codex_http_client::custom_ca", LogLevel::Warn),
    // 每次打开状态库都会为每个迁移重复同一条换行兼容提示。
    ("codex_state::migration_line_endings", LogLevel::Error),
];

/// 仅在全局级别比上限更详细时追加，避免反而放开 off/error 已关闭的记录。
/// 追加在用户的 target 覆盖之前，用户显式配置同名 target 时以用户为准。
pub(super) fn append_quiet_directives(level: LogLevel, directives: &mut String) {
    if level == LogLevel::Off {
        return;
    }
    for (target, cap) in QUIET_TARGETS {
        if level.rank() < cap.rank() {
            directives.push_str(&format!(",{target}={}", cap.directive()));
        }
    }
}
