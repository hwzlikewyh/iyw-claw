# 浏览器 MCP 页签字段说明修正设计

## 背景

内置浏览器 MCP 对页签标识使用两种命名：

- `browser_list_tabs` 返回的状态采用 camelCase，页签标识是
  `tabs[].browserTabId`。
- `browser_open` 等工具的输入 schema 采用 snake_case，参数名是 `tab_id`。

`0.1.82` 的工具说明将返回字段写成 `browser_tab_id`；当前源码虽然已在
`browser_list_tabs` 和 `browser_open` 的主说明中改为 `browserTabId`，但
`tab_id` 属性说明仍写成 `browser_tab_id`。该不一致会误导 Agent 查找不存在的
返回字段或构造错误参数。

当当前 Agent 会话看不到共享页签时，`browser_open` 返回
`BROWSER_TAB_ACCESS_DENIED`。这是用户明确授权浏览器登录态的安全门禁，不属于
本次字段说明修正范围。

## 目标

- 明确 `browser_list_tabs` 的返回字段是 `tabs[].browserTabId`。
- 明确其他浏览器工具接收的输入参数是 `tab_id`。
- 指示 Agent 先调用 `browser_list_tabs`，再将返回的 `browserTabId` 值传入
  `tab_id`。
- 保持工具名称、JSON schema 属性、运行时返回结构和授权行为不变。

## 方案

修改 `src-tauri/src/acp/delegation/tool_schema.json` 中浏览器工具说明：

1. `browser_list_tabs` 明确列出 `tabs[].browserTabId`，并说明列表为空时停止
   调用浏览器操作，要求用户共享当前页签。
2. `browser_open` 明确同页导航使用
   `{"tab_id": tabs[0].browserTabId}` 的字段映射；只有用户明确要求新页签且
   已存在共享页签时才省略 `tab_id`。
3. `browser_open.inputSchema.properties.tab_id.description` 改为说明该参数的值
   来自 `browser_list_tabs` 返回的 `tabs[].browserTabId`。
4. 其他要求 `tab_id` 的浏览器工具保持统一措辞，避免再次出现
   `browser_tab_id` 这一不存在的 JSON 字段名。

本次仅修改人类和 Agent 可读的 schema `description`。不得修改属性名、required
列表、长度限制或 `additionalProperties` 约束。

## 错误与兼容性

- MCP `tools/list` 的结构兼容，现有客户端无需适配。
- `browser_list_tabs` 返回结构保持 camelCase。
- 所有浏览器操作继续接收 snake_case 的 `tab_id`。
- `BROWSER_TAB_ACCESS_DENIED`、`isError` 和 `structuredContent.error` 的语义不变。
- 首次共享仍必须由用户在内置浏览器中明确完成，不允许 Agent 绕过授权创建
  首个共享页签。

## 验证

遵循仓库约定，不新增或运行测试。交付前执行：

- 解析 `tool_schema.json`，确认 JSON 语法有效。
- 搜索浏览器工具 schema，确认不存在将返回字段描述为 `browser_tab_id` 的文本。
- 静态核对 `BrowserTabSnapshot` 的序列化结果仍是 `browserTabId`。
- 静态核对 `agent_tools.rs` 仍从输入读取 `tab_id`。
- 执行格式检查和 `git diff --check`。

## 非目标

- 不新增浏览器授权请求工具或前端授权提示。
- 不修改页签共享范围、Agent 身份映射或会话权限模型。
- 不允许 Agent 在零共享页签状态下创建首个页签。
- 不修改浏览器帧流、导航、关闭、profile 或用户缓存逻辑。
- 不包含版本升级、安装包构建或发布。
