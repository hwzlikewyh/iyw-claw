import i18n from "@/i18n"

i18n.addResourceBundle("zh-CN", "translation", {
  canvas: { agentConnected: "当前窗口已连接", agentConnecting: "连接当前窗口…", agentDisconnected: "当前窗口未连接", openAgent: "打开当前窗口助手" },
  hostAgent: {
    name: "原助理", description: "直接连接当前窗口，任务通过左侧会话的发送流程执行。", window: "当前窗口 · {{agent}} · {{model}}", noModel: "未选择模型",
    context: "当前画布：{{title}} · 已选 {{count}} 个节点", noCanvas: "先打开一张画布，再发送任务。",
    prompt: "让原助理做什么？", placeholder: "例如：把选中的图片排成两列，并添加标题",
    send: "发送给原助理", sending: "正在发送…", sent: "已发送到原助理。请在当前会话查看回复和执行进度。",
    unavailable: "无法绑定当前窗口，请在目标会话重新打开插件。", unsupported: "当前原助理版本尚未开放窗口桥接，需更新客户端；画布和素材可继续使用。",
    retry: "重试连接", rejected: "原助理未接受这条消息，输入已保留。", failed: "发送未确认，请先在原助理会话查看是否已收到，避免重复提交。",
    syncing: "画布正在保存，请稍后再发送。", syncFailed: "画布尚未同步，请先处理画布冲突或保存错误。",
    changed: "当前画布或选择已变化，请确认后重新发送。",
    modelsLoading: "正在读取当前原助理窗口的可用模型…", modelsFailed: "读取当前窗口模型失败，请检查登录状态或重试。",
    modelsEmpty: "原助理当前没有返回可用的生成模型。", modelsRefresh: "刷新原助理模型", modelUnavailable: "当前模型已不可用，请重新选择", modelsNone: "原助理尚未提供此类模型",
  },
}, true, true)
i18n.addResourceBundle("en-US", "translation", {
  canvas: { agentConnected: "Current window connected", agentConnecting: "Connecting current window…", agentDisconnected: "Current window disconnected", openAgent: "Open current window assistant" },
  hostAgent: {
    name: "Yuan Assistant", description: "Connected directly to this window. Tasks use the conversation's normal send flow.", window: "Current window · {{agent}} · {{model}}", noModel: "No model selected",
    context: "Canvas: {{title}} · {{count}} nodes selected", noCanvas: "Open a canvas before sending a task.",
    prompt: "What should Yuan Assistant do?", placeholder: "For example: arrange the selected images in two columns and add a title",
    send: "Send to Yuan Assistant", sending: "Sending…", sent: "Sent to Yuan Assistant. View replies and progress in the current conversation.",
    unavailable: "Cannot bind this window. Reopen the plugin in the target conversation.", unsupported: "This client does not expose the window bridge. Update Yuan Assistant; canvas and assets remain available.",
    retry: "Retry connection", rejected: "The host rejected this message. Your input has been kept.", failed: "Delivery was not confirmed. Check the conversation before resending to avoid duplicates.",
    syncing: "The canvas is being saved. Try again shortly.", syncFailed: "The canvas has not synced. Resolve any save error or conflict first.",
    changed: "The canvas or selection changed. Review it before sending again.",
    modelsLoading: "Reading models available to this Yuan Assistant window…", modelsFailed: "Could not read this window's models. Check sign-in and retry.",
    modelsEmpty: "Yuan Assistant returned no generation models.", modelsRefresh: "Refresh host models", modelUnavailable: "This model is unavailable. Choose another model.", modelsNone: "The host has not provided models for this capability",
  },
}, true, true)
