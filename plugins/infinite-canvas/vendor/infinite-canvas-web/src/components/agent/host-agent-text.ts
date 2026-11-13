import i18n from "@/i18n"

i18n.addResourceBundle("zh-CN", "translation", {
  canvas: { agentConnected: "原助理已连接", agentConnecting: "原助理连接中", agentDisconnected: "原助理未连接", openAgent: "打开原助理" },
  hostAgent: {
    name: "原助理", description: "发送到原助理当前会话，画布修改会自动同步。",
    context: "当前画布：{{title}} · 已选 {{count}} 个节点", noCanvas: "先打开一张画布，再发送任务。",
    prompt: "让原助理做什么？", placeholder: "例如：把选中的图片排成两列，并添加标题",
    send: "发送给原助理", sending: "正在发送…", sent: "已发送到原助理。请在当前会话查看回复和执行进度。",
    unavailable: "无法连接原助理，请重新打开插件或重试连接。", unsupported: "当前宿主未提供会话消息通道。",
    retry: "重试连接", rejected: "原助理未接受这条消息，输入已保留。", failed: "发送未确认，请先在原助理会话查看是否已收到，避免重复提交。",
    syncing: "画布正在保存，请稍后再发送。", syncFailed: "画布尚未同步，请先处理画布冲突或保存错误。",
    changed: "当前画布或选择已变化，请确认后重新发送。",
    generating: "原助理正在当前会话处理 {{count}} 个生成请求，完成后会自动回填结果。",
    modelsLoading: "正在从原助理同步模型列表，请在当前会话查看进度…", modelsFailed: "无法同步原助理模型，请检查当前会话后重试。",
    modelsEmpty: "原助理当前没有返回可用的生成模型。", modelsRefresh: "刷新原助理模型", modelUnavailable: "当前模型已不可用，请重新选择", modelsNone: "原助理尚未提供此类模型",
  },
}, true, true)
i18n.addResourceBundle("en-US", "translation", {
  canvas: { agentConnected: "Yuan Assistant connected", agentConnecting: "Connecting to Yuan Assistant", agentDisconnected: "Yuan Assistant disconnected", openAgent: "Open Yuan Assistant" },
  hostAgent: {
    name: "Yuan Assistant", description: "Send to the current Yuan Assistant conversation. Canvas edits sync automatically.",
    context: "Canvas: {{title}} · {{count}} nodes selected", noCanvas: "Open a canvas before sending a task.",
    prompt: "What should Yuan Assistant do?", placeholder: "For example: arrange the selected images in two columns and add a title",
    send: "Send to Yuan Assistant", sending: "Sending…", sent: "Sent to Yuan Assistant. View replies and progress in the current conversation.",
    unavailable: "Cannot connect to Yuan Assistant. Reopen the plugin or retry the connection.", unsupported: "This host does not provide conversation messaging.",
    retry: "Retry connection", rejected: "The host rejected this message. Your input has been kept.", failed: "Delivery was not confirmed. Check the conversation before resending to avoid duplicates.",
    syncing: "The canvas is being saved. Try again shortly.", syncFailed: "The canvas has not synced. Resolve any save error or conflict first.",
    changed: "The canvas or selection changed. Review it before sending again.",
    generating: "Yuan Assistant is handling {{count}} generation requests in the current conversation. Results will appear here when complete.",
    modelsLoading: "Syncing models from Yuan Assistant. View progress in the current conversation…", modelsFailed: "Could not sync models. Check the host conversation and retry.",
    modelsEmpty: "Yuan Assistant returned no generation models.", modelsRefresh: "Refresh host models", modelUnavailable: "This model is unavailable. Choose another model.", modelsNone: "The host has not provided models for this capability",
  },
}, true, true)
