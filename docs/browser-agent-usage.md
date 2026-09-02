# Agent 浏览器调用与 OpenCLI 回退

## 浏览器优先级

对于网页和公开数据任务，已有可靠专用 API 或直接数据源且能完整满足请求时可以先用；
出现无数据、数据不完整、动态渲染、登录态依赖或结果无法验证时，必须转入 iyw-claw
内置托管浏览器，不能直接报告“拿不到”，也不能先切换其他浏览器。所有浏览器方案中，
内置 `agent-browser` 始终排第一。

Agent 在浏览器工作前读取内置 `agent-browser` Skill。该 Skill 基于
`agent-browser-plus-1.0.2` 的完整操作说明，并按 iyw-claw 固定 sidecar、固定页签、
持久 profile、用户接管和安全策略进行了适配。

## 默认调用链

用户没有指定浏览器时，Agent 使用 iyw-claw 内置托管浏览器：

```text
browser_list_tabs
  -> browser_open
  -> browser_read / browser_snapshot
  -> browser_click / browser_fill / browser_press / browser_scroll / browser_wait / browser_command
  -> fresh browser_snapshot
  -> 业务结果验证
```

MCP 工具由 `src-tauri/src/acp/delegation/tool_schema.json` 广告，经内置 HTTP MCP gateway 和 delegation listener 分发到 `BrowserSessionManager::execute_agent_tool`。交互动作最终使用固定页签的 `agent-browser --cdp ... --pin-tab` 控制器，不是 Tauri WebView 的 DOM 事件。

`browser_read` 是网页数据读取入口；`browser_command` 是高级受管入口。后者把
`command` 和逐项 `arguments` 直接作为进程参数传给固定页签控制器，不经过 shell，
支持参考 Skill 中没有独立 MCP 工具的操作：

- 导航与读取：back、forward、reload、read、get、is、find。
- 页面交互：dblclick、focus、hover、type、键盘、checkbox、select、drag、upload、download。
- 等待与定位：text/URL/load/function 等 wait 变体、scrollintoview。
- 输出与调试：PDF、trace、profiler、console、errors、highlight、diff。
- 页面环境：viewport、device、geo、media、mouse、frame、dialog、eval。
- 高级检查：network、cookies/storage/state、a11y、vitals、React、pushstate。

open、close、tab/window、session/profile、record、install/upgrade、MCP、plugin、dashboard 和 chat
不会通过高级入口执行：open/close/tab/window/session/profile 由 iyw-claw 自己维护
页签、窗口和运行时身份；install/upgrade/MCP/plugin/dashboard/chat 属于 sidecar 控制面；
record 会创建新 context，目前也会破坏固定页签身份。
cookies、storage、state、headers、credentials、clipboard
和 eval 均视为敏感操作，只能按当前 schema 和明确任务使用，不得写入日志或用户报告。

## 内置浏览器引用规则

- `browser_list_tabs` 返回的 `tabs[].browserTabId` 才是其他工具的 `tab_id`。
- `browser_snapshot` 生成的 `@eN` 只对应当时的页面状态。
- 导航、SPA 路由变化、弹窗、列表刷新或一次写操作后，必须重新 snapshot。
- 动作失败时，先判断是 stale reference、selector 问题还是 runtime/session 问题。stale/locator 类问题对同一预期动作总共只有一次恢复机会：重新 snapshot，使用一个新引用或修正后的 locator；不能通过轮换 locator 扩大重试预算。
- runtime/session/daemon/observer 或 timeout 类失败只检查一次当前状态，不要求重新 snapshot；确认内置路由不可用后，默认停止并报告原因。只有用户明确指定 OpenCLI，或已开启外部浏览器设置时，才允许切换。写操作如果返回 `effectMayHaveOccurred`，先读取页面状态确认结果，再决定是否继续。

## 用户接管边界

只有登录所需的用户凭据、MFA/OTP、CAPTCHA、设备批准、安全支付确认、内置能力确实
无法完成的交互或明确的最终人工复核，才调用 `browser_request_user_action`。普通 selector
失败、陈旧引用、页面慢、等待条件遗漏或常规数据提取失败不能转交用户；Agent 先做一次
新 snapshot 和定位修正。用户操作结束后继续复用同一托管页签并验证业务结果。

## 错误判断

后端会把控制器结果转换为浏览器错误：

| 错误类别 | 含义 | Agent 行为 |
| --- | --- | --- |
| `BROWSER_SNAPSHOT_STALE` | `@eN` 或页面代际已过期 | 重新 snapshot，换新引用 |
| `BROWSER_INVALID_ARGUMENT` | selector 无效、找不到或不唯一 | 用 snapshot/find 改进定位，不归因于 runtime |
| `BROWSER_TAB_GONE` | pinned tab 已关闭或目标丢失 | 重新列出页签，必要时新建页签 |
| `BROWSER_OPERATION_TIMEOUT` | 控制器动作超时 | 检查页面状态一次，默认停止；只有用户明确授权时才切换外部浏览器 |
| `BROWSER_CONTROL_CHANGED` | 点击点被弹窗、横幅或其他元素遮挡 | 先处理遮挡元素或刷新状态，不要归因于 runtime |
| `BROWSER_RUNTIME_UNAVAILABLE` | daemon/session/runtime/observer 可能不可用，或控制器返回未知错误 | 不要把它解释成“点击功能被禁用”；完成一次状态检查后默认停止，禁止自动回退 |

`BROWSER_RUNTIME_UNAVAILABLE` 是兼容性错误码，不等于已证明浏览器内核退出。排查时结合 `browser_list_tabs`、runtime 日志和页签状态判断。后端只向 Agent 返回白名单内的稳定控制器 code，未知 code 统一隐藏。

## 外部浏览器流程

OpenCLI 是独立的外部浏览器方案，iyw-claw 不会自动启动、绑定或切换到它。
只有用户在当前任务中明确指定，或设置中明确开启外部浏览器，才允许执行本节流程。

1. 读取当前安装的 `opencli-browser` Skill；以实际 Skill 文档为准，不猜参数。
2. 执行：

   ```bash
   opencli doctor
   ```

   doctor 失败时，先处理 Chrome、扩展或调试端口前置条件。

3. 为一条连续任务使用稳定 session：

   ```bash
   opencli browser sales-report bind
   opencli browser sales-report state
   ```

   已有用户登录页签使用 `bind`；需要独立页签时使用 `open <url>`。

4. 先 `state` 或 `find`，再执行动作。优先使用当前 numeric ref；CSS selector 必须唯一。
5. 每次写操作检查结构化结果中的 `matches_n`、`match_level` 和成功字段。页面变化后重新 `state`。
6. 通过 URL、页面文本、字段值或网络结果验证业务动作完成；不要只根据点击返回成功就宣称业务完成。
7. 自有 session 用 `close`；绑定用户页签只用 `unbind`，不关闭用户窗口。

OpenCLI 的 session/page identity 与内置浏览器的 `browserTabId` 不同，不要交叉传递。遇到登录墙、验证码或人工接管时暂停自动化，让用户完成处理后复用同一个绑定 session。

## 安装和运行时

### 内置浏览器引擎

桌面应用安装包不包含 Chrome for Testing。首次启动完成后，应用在后台从 Fusion
受管组件服务预下载 `browser-engine`，过程不打开浏览器窗口、不弹窗，也不唤醒系统
Chrome/Edge。下载、摘要、签名、解压、文件布局和受管 marker 校验全部通过后才会激活
版本；启动阶段不会执行浏览器程序。失败只保留已有的 last-known-good 版本。

用户首次打开内置浏览器时，如果后台下载仍在进行，前台请求会等待同一个安装任务，
不会创建第二个下载。离线或未登录时，后台失败保持内部状态；用户主动重试时才显示
可操作的错误。受管引擎使用 iyw-claw 自有 profile，不绑定正在运行的普通浏览器。

OpenCLI 由 Internet Tools bootstrap 安装到托管 Node prefix。当前版本由 `src-tauri/src/commands/internet_tools/types.rs` 的 `OPENCLI_VERSION` 固定。`src-tauri/src/acp/connection.rs` 把托管 bin 目录加入 Agent 和 ACP terminal 的 PATH；`src-tauri/src/commands/internet_tools.rs` 提供 bin 目录和 `MCPORTER_CONFIG`。安装完成后，OpenCLI 自带的 `opencli-*` Skills 会同步到 central Skill 目录。

Agent 不应向用户暴露托管绝对路径、token、cookies 或完整命令细节；只在工具缺失、doctor 失败或需要人工登录时报告具体前置条件。
