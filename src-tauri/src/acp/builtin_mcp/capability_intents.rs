#[derive(Debug, Clone, Copy)]
pub(super) struct CapabilityIntentMetadata {
    pub aliases: &'static [&'static str],
    pub intent_terms: &'static [&'static str],
    pub negative_terms: &'static [&'static str],
    pub when_to_use: &'static str,
}

const EMPTY: &[&str] = &[];

macro_rules! metadata {
    ($aliases:expr, $terms:expr, $when:expr) => {
        CapabilityIntentMetadata {
            aliases: $aliases,
            intent_terms: $terms,
            negative_terms: EMPTY,
            when_to_use: $when,
        }
    };
}

// Keep routing hints concise and per-tool. Runtime schemas, stable IDs, and
// availability remain owned by the capability catalog.
#[rustfmt::skip]
const SPECIAL_METADATA: &[(&str, CapabilityIntentMetadata)] = &[
    ("list_scheduled_task_projects", metadata!(&["list scheduled task projects", "列出定时任务项目", "计划任务项目"], &["list", "列出", "scheduled", "定时", "project", "项目"], "Use to inspect available scheduled-task projects before selecting one.")),
    ("list_scheduled_tasks", metadata!(&["list scheduled tasks", "列出定时任务", "查看计划任务"], &["list", "列出", "scheduled", "定时", "task", "任务"], "Use to inspect existing scheduled tasks.")),
    ("create_scheduled_task", metadata!(&["create scheduled task", "创建定时任务", "新建计划任务"], &["create", "创建", "scheduled", "定时", "task", "任务"], "Use only when the user asks to create an automation and its schedule and prompt are known.")),
    ("update_scheduled_task", metadata!(&["update scheduled task", "修改定时任务", "更新计划任务"], &["update", "修改", "scheduled", "定时", "task", "任务"], "Use to change an identified scheduled task after reading its current details.")),
    ("delete_scheduled_task", metadata!(&["delete scheduled task", "删除定时任务", "移除计划任务"], &["delete", "删除", "scheduled", "定时", "task", "任务"], "Use only after the user confirms the exact scheduled task to delete.")),
    ("browser_list_tabs", metadata!(&["list browser tabs", "列出浏览器标签页", "查看打开的网页"], &["list", "列出", "browser", "浏览器", "tab", "标签页"], "Use to inspect existing managed-browser tabs before opening or reusing one.")),
    ("browser_open", metadata!(&["open browser page", "打开网页", "浏览器打开网址"], &["open", "打开", "browser", "浏览器", "page", "网页"], "Use to navigate the managed browser to a user-requested URL.")),
    ("browser_snapshot", metadata!(&["browser snapshot", "获取网页快照", "读取页面"], &["snapshot", "快照", "browser", "浏览器", "read", "读取"], "Use to inspect the current managed-browser DOM before an action.")),
    ("browser_read", metadata!(&["read browser page", "读取网页数据", "浏览器获取公开数据"], &["read", "读取", "browser", "浏览器", "web", "网页", "data", "数据"], "Use as the managed-browser data route when a direct API, search result, or static fetch is unavailable, incomplete, dynamically rendered, or requires the signed-in browser profile.")),
    ("browser_click", metadata!(&["click browser element", "点击网页元素", "浏览器点击"], &["click", "点击", "browser", "浏览器", "element", "元素"], "Use to click an element identified by a fresh managed-browser snapshot.")),
    ("browser_fill", metadata!(&["fill browser field", "填写网页表单", "浏览器输入"], &["fill", "填写", "browser", "浏览器", "form", "表单"], "Use to enter user-provided text into an identified managed-browser field.")),
    ("browser_press", metadata!(&["press browser key", "按网页键盘按键", "浏览器按键"], &["press", "按键", "browser", "浏览器", "keyboard", "键盘"], "Use to send a keyboard key or shortcut to the managed browser.")),
    ("browser_scroll", metadata!(&["scroll browser page", "滚动网页", "浏览器滚动"], &["scroll", "滚动", "browser", "浏览器", "page", "页面"], "Use to scroll the managed-browser page when the target is outside the current view.")),
    ("browser_wait", metadata!(&["wait for browser page", "等待网页", "浏览器等待"], &["wait", "等待", "browser", "浏览器", "page", "页面"], "Use to wait for a specific managed-browser state or element.")),
    ("browser_screenshot", metadata!(&["browser screenshot", "网页截图", "浏览器截图"], &["screenshot", "截图", "browser", "浏览器", "image", "图片"], "Use when the user needs a visual screenshot of the managed browser.")),
    ("browser_close_tab", metadata!(&["close browser tab", "关闭浏览器标签页", "关闭网页"], &["close", "关闭", "browser", "浏览器", "tab", "标签页"], "Use only to close an identified managed-browser tab when requested or required for cleanup.")),
    ("browser_command", metadata!(&["advanced browser command", "浏览器高级操作", "完整网页自动化"], &["advanced", "高级", "browser", "浏览器", "command", "操作", "automation", "自动化"], "Use after reading the agent-browser Skill when the dedicated managed browser tools do not expose a required page operation.")),
    ("browser_request_user_action", metadata!(&["request browser user action", "请求用户操作浏览器", "让用户操作网页"], &["request", "请求", "user", "用户", "action", "操作", "browser", "浏览器"], "Use only for a human-only browser step such as user-held credentials, MFA, CAPTCHA, device approval, secure payment confirmation, or an interaction unavailable to the managed browser.")),
    ("browser_present", metadata!(&["present browser window", "展示浏览器窗口", "展示网页成果", "打开界面给用户看"], &["present", "展示", "browser", "浏览器", "window", "窗口", "ui", "界面"], "Use proactively to show the user a completed web UI, local service, HTML preview, or other visual browser result in a detached window.")),
    ("browser_close_window", metadata!(&["close browser window", "关闭浏览器显示窗口", "收起浏览器窗口"], &["close", "关闭", "browser", "浏览器", "window", "窗口"], "Use after a visible browser hand-off is complete to close its detached window while preserving the tab and sign-in state.")),
    ("present_task_files", metadata!(&["task artifacts", "任务成果", "提交成果", "交付文件", "present final files", "交付最终文件"], &["present", "提交", "artifact", "成果", "deliver", "交付"], "Use to register every final user-facing file, directory, or public URL in the current conversation Artifacts.")),
    ("delegate_to_agent", metadata!(&["delegate task", "委派任务", "分派智能体"], &["delegate", "委派", "agent", "智能体"], "Use for a bounded, independently verifiable task that another Agent can execute.")),
    ("get_delegation_status", metadata!(&["delegation status", "查看委派状态", "查询子任务"], &["status", "状态", "delegation", "委派", "subtask", "子任务"], "Use to inspect an identified delegated task.")),
    ("cancel_delegation", metadata!(&["cancel delegation", "取消委派", "停止子任务"], &["cancel", "取消", "delegation", "委派", "subtask", "子任务"], "Use only when the user requests cancellation of an identified delegated task.")),
    ("check_user_feedback", metadata!(&["check user feedback", "查看用户反馈", "读取反馈"], &["check", "查看", "feedback", "反馈", "user", "用户"], "Use at sensible checkpoints during a long-running task when feedback may change the work.")),
    ("ask_user_question", metadata!(&["ask user question", "询问用户", "向用户提问"], &["ask", "询问", "question", "问题", "user", "用户"], "Use when a required decision or input cannot be discovered safely from context.")),
    ("get_session_info", metadata!(&["session info", "会话信息", "查看会话"], &["read", "读取", "session", "会话"], "Use to inspect a referenced iyw-claw session, not to guess another session's state.")),
    ("transcribe_audio", metadata!(&["complex audio transcription", "复杂音频转写", "异步语音转写", "会议录音转写"], &["complex", "复杂", "async", "异步", "meeting", "会议", "speaker", "说话人", "resumable", "可恢复"], "Use for complex, durable, multi-speaker, oversized, or resumable transcription. It creates an asynchronous task that can be queried by job ID.")),
    ("transcribe_audio_flash", metadata!(&["flash audio transcription", "快速音频转写", "快速语音转文字", "普通音频转写"], &["flash", "快速", "immediate", "立即", "short", "短音频", "audio", "音频"], "Prefer for ordinary short audio that needs immediate text and fits the 100 MiB and 2-hour flash limits; not for speaker diarization or resumable work.")),
    ("query_audio_transcription", metadata!(&["query audio transcription", "查询音频转写", "读取转写结果"], &["query", "查询", "transcription", "转写", "audio", "音频"], "Use to read the result of an identified audio-transcription task.")),
    ("show_image", metadata!(&["show image", "展示图片", "显示生成图片"], &["show", "展示", "image", "图片", "display", "显示"], "Use to present an existing or generated image when the direct display route is available.")),
    ("analyze_image", metadata!(&["analyze image", "分析图片", "理解图片", "识别图像"], &["analyze", "分析", "image", "图片", "understand", "理解"], "Use first when the task requires understanding or judging image content; do not use it to generate images.")),
    ("get_current_user_profile", metadata!(&["user profile", "用户资料", "用户姓名", "用户昵称", "称呼", "查我是谁"], &["profile", "资料", "name", "姓名", "identity", "身份"], "Use to read the current account display profile, not stored memory.")),
    ("append_user_memory", metadata!(&["remember user fact", "记住用户事实", "保存用户偏好", "长期记忆"], &["remember", "记住", "append", "保存", "memory", "记忆"], "Use only after an explicit request to retain a durable user fact or preference.")),
    ("propose_user_memory", metadata!(&["propose memory", "候选记忆", "记录用户纠正", "建议保存偏好"], &["propose", "候选", "memory", "记忆", "correction", "纠正"], "Use for a conservative reusable correction, preference, or fact that is not explicitly confirmed for durable storage.")),
    ("memory_recall", metadata!(&["recall memory", "检索记忆", "查历史记忆", "查询之前的决定"], &["recall", "检索", "memory", "记忆", "history", "历史"], "Use when the task depends on prior decisions, preferences, repeated workflows, or earlier context; never use it to read account identity.")),
    ("read_user_memory_documents", metadata!(&["read user memory documents", "读取用户记忆文件", "读取用户画像", "读取用户行为准则"], &["read", "读取", "memory", "记忆", "profile", "画像", "soul", "准则"], "Use only when the task needs the current authoritative contents of selected user-memory documents; request the smallest relevant set.")),
    ("list_user_memory_candidates", metadata!(&["list memory candidates", "列出候选记忆", "查看学习候选"], &["list", "列出", "candidate", "候选", "memory", "记忆"], "Use to inspect host-managed learning candidates before resolving or deleting one.")),
    ("resolve_user_memory_candidate", metadata!(&["resolve memory candidate", "处理候选记忆", "确认或拒绝记忆"], &["resolve", "处理", "candidate", "候选", "confirm", "确认", "reject", "拒绝"], "Use with an exact candidate id and revision after reading the current candidate page.")),
    ("delete_user_memory_candidate", metadata!(&["delete memory candidate", "删除候选记忆"], &["delete", "删除", "candidate", "候选", "memory", "记忆"], "Use only for an exact terminal candidate after reading its current revision.")),
    ("get_user_memory_harvest_status", metadata!(&["memory harvest status", "记忆收获状态", "记忆队列状态"], &["status", "状态", "harvest", "收获", "queue", "队列", "memory", "记忆"], "Use to inspect host TurnComplete memory-harvest backlog and failure counts.")),
    ("rescan_user_memory_harvest", metadata!(&["rescan memory harvest", "重扫记忆收获", "重排记忆队列"], &["rescan", "重扫", "harvest", "收获", "memory", "记忆"], "Preview first; execute only after the user explicitly requests a rescan.")),
    ("rebuild_user_memory_candidate_index", metadata!(&["rebuild memory candidate index", "重建候选记忆索引"], &["rebuild", "重建", "index", "索引", "candidate", "候选"], "Preview first; execute only for an explicit index-repair request.")),
    ("get_user_memory_settings", metadata!(&["memory settings", "记忆设置", "记忆健康"], &["settings", "设置", "health", "健康", "memory", "记忆"], "Use for a safe memory capability and health summary without exposing private paths.")),
    ("update_user_memory_documents", metadata!(&["update memory documents", "编辑记忆文档", "修改用户记忆"], &["update", "修改", "edit", "编辑", "document", "文档", "memory", "记忆"], "Use only after reading the target documents and sending exact revision/eTag guarded patches.")),
    ("correct_user_memory", metadata!(&["correct user memory", "修正用户记忆", "纠正记忆条目"], &["correct", "修正", "纠正", "memory", "记忆", "entry", "条目"], "Use for an exact existing memory correction so the host can normalize candidate references transactionally.")),
    ("list_message_channels", metadata!(&["list message channels", "列出消息渠道", "查看渠道"], &["list", "列出", "channel", "渠道", "message", "消息"], "Use to inspect configured message channels.")),
    ("save_message_channel", metadata!(&["save message channel", "保存消息渠道", "配置渠道"], &["save", "保存", "channel", "渠道", "message", "消息"], "Use to create or update a configured message channel when its schema and authorization are available.")),
    ("delete_message_channel", metadata!(&["delete message channel", "删除消息渠道", "移除渠道"], &["delete", "删除", "channel", "渠道", "message", "消息"], "Use only after the user confirms the exact configured channel to delete.")),
    ("manage_channel_credential", metadata!(&["manage channel credential", "管理渠道凭据", "配置消息凭据"], &["credential", "凭据", "channel", "渠道", "manage", "管理"], "Use only through a schema that explicitly supports secure credential handling; never request secrets in ordinary chat.")),
    ("operate_message_channel", metadata!(&["operate message channel", "操作消息渠道", "启停消息渠道"], &["operate", "操作", "channel", "渠道", "message", "消息"], "Use to perform a supported lifecycle operation on an identified message channel.")),
    ("list_channel_targets", metadata!(&["list channel targets", "列出渠道目标", "查看消息接收方"], &["list", "列出", "target", "目标", "channel", "渠道"], "Use to inspect available targets for an identified message channel before sending.")),
    ("list_channel_messages", metadata!(&["list channel messages", "列出渠道消息", "查询历史消息"], &["list", "列出", "message", "消息", "history", "历史"], "Use to read messages from an identified channel and target when requested.")),
    ("send_channel_messages", metadata!(&["send channel message", "发送消息", "发企业微信消息", "发送渠道消息"], &["send", "发送", "message", "消息", "channel", "渠道"], "Use to send a message through a configured channel after resolving the exact target and content.")),
    ("manage_channel_settings", metadata!(&["manage channel settings", "管理渠道设置", "修改消息渠道配置"], &["settings", "设置", "channel", "渠道", "manage", "管理"], "Use to change supported settings for an identified message channel.")),
];

pub(super) fn intent_metadata(tool_name: &str) -> Option<CapabilityIntentMetadata> {
    SPECIAL_METADATA
        .iter()
        .find_map(|(name, metadata)| (*name == tool_name).then_some(*metadata))
}

pub(super) fn validate_intent_metadata<'a>(
    tool_names: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let missing = tool_names
        .into_iter()
        .filter(|name| intent_metadata(name).is_none())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("intent metadata missing for tools: {missing:?}"))
    }
}
