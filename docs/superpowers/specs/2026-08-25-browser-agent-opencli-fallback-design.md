# 内置浏览器诊断与 OpenCLI 优先路由设计

日期：2026-08-25

## 目标

当用户要求使用浏览器时，统一 Browser MCP 同时检查用户的真实 Chrome/OpenCLI 与
iyw-claw 内置浏览器，默认优先 OpenCLI 以复用用户已有登录态。只有 OpenCLI 明确报告
登录、MFA/OTP、CAPTCHA、设备批准、安全确认、人工复核或需要用户接管的交互时，才
切换到内置浏览器；Chrome、扩展、CDP、网络、超时、selector 和未知错误直接失败。

本设计覆盖四个结果：

1. Agent 能区分内置浏览器的 stale reference、页签失效、控制器不可用和操作超时。
2. Agent 不会在同一个失效 `@eN` 引用上无限重试。
3. OpenCLI 的调用契约、登录态绑定、状态刷新和写操作验证对 Agent 可发现且与实际安装版本一致。
4. OpenCLI 与内置浏览器在 MCP 层统一为一个入口，任务内 provider 锁定且只允许
   `opencli -> managed` 的人工操作切换。

## 当前调用链

内置浏览器工具由 `delegation/tool_schema.json` 暴露，经 HTTP MCP gateway、delegation listener 和 `BrowserSessionManager::execute_agent_tool` 分发。`browser_click` 在 `agent_tool_actions.rs` 中转为 `agent-browser --cdp <url> --pin-tab click <selector>`；命令输出由 `command_output.rs` 解析后再投影为 Agent 状态。

当前 `command_output.rs` 仅把 `tab_gone`、`dialog_pending` 映射为专用错误，其余 CLI 错误统一映射为 `BrowserRuntimeUnavailable`，并丢弃原始 `code` 与 `message`。因此 Agent 不能仅凭现有错误码判断 runtime 是否真的退出。

OpenCLI 由 iyw-claw 的 Internet Tools bootstrap 安装到托管 Node prefix，当前固定版本由 `commands/internet_tools/types.rs` 管理。`connection.rs` 会把托管 OpenCLI bin 目录加入 Agent 和 ACP terminal 的 PATH；安装后会把 OpenCLI 自带的 `opencli-*` Skills 同步到 central Skill 目录。OpenCLI Skill 的实际契约是 `doctor`、稳定 session、`bind`/`open`、`state`/`find`、结构化 action envelope 和写操作后的复核。

## Agent 行为契约

### 默认路径

- 用户只说“浏览器”时，先使用当前 MCP surface 实际广告的 `browser_*` 工具和 schema。
- 先调用 `browser_list_tabs`，复用 `activeTabId`；只有用户明确要求新页签才使用 `new_tab=true`。
- `browser_snapshot` 返回的 `@eN` 只能用于同一页面状态。导航、SPA 路由变化、弹窗、列表刷新或一次写操作后必须重新 snapshot。
- 点击或填充失败时，先判断错误类别；若可能是引用陈旧，只允许重新 snapshot 后重试一次，不复用旧引用。

### 进入内置浏览器人工回退

统一 Browser MCP 仅在 OpenCLI 返回以下人工操作分类时切换到内置浏览器：

- 登录、MFA/OTP、CAPTCHA、设备批准或安全确认；
- 需要用户持有凭据或明确人工复核；
- OpenCLI 无法完成但明确要求用户接管的交互。

selector 不存在、selector 不明确或 stale reference 本身不应直接归因于 runtime；Agent 应先刷新状态或改用更稳定的定位方式。对于有副作用的动作，错误返回 `effect_may_have_occurred` 时不得盲目重放，必须先检查页面状态。

### OpenCLI 优先流程

1. 读取已安装的 `opencli-browser` Skill；不要依据记忆猜命令或参数。
2. 运行 `opencli doctor`。初始未安装时可以使用内置浏览器；已安装但 doctor
   报告 Chrome、扩展、debug/CDP、网络、超时或未知错误时直接返回结构化失败。
3. 复用同一个稳定 session 名。已有用户登录页签用 `opencli browser <session> bind`；需要独立页签时用 `opencli browser <session> open <url>`。
4. 每次动作前运行 `state` 或 `find`；优先使用当前 numeric ref，必要时使用唯一 CSS/semantic locator。
5. 点击、输入、选择等写操作必须检查结构化 envelope 的 `matches_n`、`match_level` 和成功字段；`reidentified` 或页面发生变化时重新 `state`。
6. 导航、提交或 SPA 路由变化后重新 `state`，并用页面文本、URL、网络结果或字段值验证业务结果。
7. 完成后关闭自有 session；绑定用户页签只执行 `unbind`，不得关闭用户窗口或页签。

OpenCLI session/page identity 与内置页签 ID 不交叉。人工回退只做一次性、同源、
内存态 auth handoff，短 TTL 导入有限 Cookie 与 storage；不复制密码、完整 profile、
扩展或跨站凭据，也不写日志。

## 后端诊断增强

本设计允许修改内置浏览器错误投影，并新增受控 OpenCLI runner/provider：

- 保留 CLI 错误的有限、脱敏后的 `code` 和 message 摘要，避免只返回 `The browser controller rejected the operation`。
- 至少区分 `stale_ref`/`selector_not_found`/`selector_ambiguous`、`tab_gone`、`session/runtime_unavailable`、`timeout` 和未知控制器错误。
- 保留现有 `BrowserErrorCode` 兼容性；新增分类优先使用现有专用错误，无法兼容时在结构化 context 或稳定错误细节中表达，不修改 MCP 工具名称和必填字段。
- 日志记录 connection、conversation、turn generation、tab id、operation、原始错误 code、结果分类和是否建议回退；不记录 selector 的完整敏感值、Cookie、token、密码或完整 payload。

## Tool schema 与内置提示词

更新 `delegation/tool_schema.json` 的浏览器说明：

- 明确 snapshot ref 在页面变化后失效。
- 明确点击失败先 fresh snapshot 重试一次，连续失败不要循环重试。
- 明确只有人工操作类 OpenCLI 失败可切换到内置浏览器，其他失败直接停止。
- 明确 OpenCLI 是外部真实 Chrome 路径，必须先 `doctor`，再 `bind/open`，动作前 `state/find`，动作后验证。
- 保留“使用当前实际广告的工具和 schema，不猜 namespace/参数”的 MCP 通用规则。

更新 `builtin_agent_prompt.rs` 的浏览器段：内置浏览器仍为默认；定义一次 fresh snapshot 重试门槛；说明 OpenCLI 回退是 Agent 自主行为；说明 OpenCLI 不可用时应报告具体前置条件，不把它描述成内置点击能力故障。

## 安全与边界

- OpenCLI 通过受控 runner 从 Rust MCP 调用；输出有界，不记录敏感 payload，失败分类稳定。
- 不把 OpenCLI daemon 的本地端口暴露给网络；只允许托管 Skill 的本地命令路径。
- 遇到登录墙、验证码或人工接管时暂停自动动作，要求用户在真实浏览器中处理，再复用同一绑定 session。
- 读操作失败可有限重试；写操作若效果未知，先验证再决定是否继续。
- 不引入第三套浏览器引擎；不复制完整 Chrome profile；仅人工回退时做同源短期 handoff。

## 验证

遵循当前仓库 AGENTS 规则，不默认运行桌面测试、集成测试或 E2E。交付前执行：

- 解析 `tool_schema.json`，核对每个浏览器工具说明包含 fresh snapshot 和回退边界。
- 静态核对 `builtin_agent_prompt.rs` 的默认浏览器和 OpenCLI 回退规则与 schema 一致。
- 静态核对 `command_output.rs` 的原始错误分类不泄露敏感 payload，且未知错误仍保持兼容错误码。
- 核对 OpenCLI 版本、托管路径、PATH 注入和 central Skill 同步调用链。
- `git diff --check`，并进行 Rust/JSON 定向静态审查。
- 运行时风险单独报告：未在真实数企页面上验证内置浏览器或 OpenCLI 的完整业务动作，不把静态验证当成 E2E 成功。
