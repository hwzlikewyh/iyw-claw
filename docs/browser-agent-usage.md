# Agent 浏览器调用与 OpenCLI 优先

## 浏览器优先级

对于网页和公开数据任务，已有可靠专用 API 或直接数据源且能完整满足请求时可以先用。
用户明确要求“用浏览器”或任务需要网页交互时，统一调用 `browser` MCP 工具；它会同时
检查用户已连接的 Chrome/OpenCLI 和 iyw-claw 内置浏览器，默认优先 OpenCLI，以复用
用户已有 Chrome 登录态。旧的 `browser_*` 工具仍可用，但只是兼容别名。

Agent 按统一工具选中的 provider 读取对应 Skill：OpenCLI 路径读取
`opencli-browser`，内置浏览器路径读取 `agent-browser`。后者基于
`agent-browser-plus-1.0.2` 的完整操作说明，并按 iyw-claw 固定 sidecar、固定页签、
持久 profile、用户接管和安全策略进行了适配。

## 默认调用链

用户要求使用浏览器时，Agent 使用统一入口：

```text
browser(action=list_tabs)
  -> browser(action=open)
  -> browser(action=read|snapshot)
  -> browser(action=click|fill|press|scroll|wait|advanced)
  -> fresh browser(action=snapshot)
  -> 业务结果验证
```

MCP 工具由 `src-tauri/src/acp/delegation/tool_schema.json` 广告，经内置 HTTP MCP gateway 和
delegation listener 分发到 `BrowserSessionManager::execute_agent_tool`。统一入口先运行
OpenCLI 的 doctor 和真实 Chrome Browser Bridge；若 OpenCLI 不可用，返回具体错误，不会
因为 Chrome、扩展、CDP、网络、超时、selector 或未知错误自动启动内置浏览器。

OpenCLI 成功后，一个任务会锁定同一个 OpenCLI session。只有 OpenCLI 明确报告登录、MFA/OTP、
CAPTCHA、设备批准、安全确认、人工复核或确实需要用户接管的交互，才会切到内置浏览器；
切换后任务固定使用内置浏览器，不会再切回 OpenCLI。
普通 OpenCLI 自动化使用 background window，避免抢占用户当前前台窗口；需要人工处理时才
创建并展示 iyw-claw 内置浏览器页签。

`browser(action=read)` 是网页数据读取入口；`browser(action=advanced)` 是高级受管入口。后者把
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

- `browser(action=list_tabs)` 返回的 `browserTabId` 才是后续调用的 `tab_id`。
- `browser(action=snapshot)` 生成的 `@eN` 只对应当时的页面状态。
- 导航、SPA 路由变化、弹窗、列表刷新或一次写操作后，必须重新 snapshot。
- 动作失败时，先判断是 stale reference、selector 问题还是 runtime/session 问题。stale/locator 类问题对同一预期动作总共只有一次恢复机会：重新 snapshot，使用一个新引用或修正后的 locator；不能通过轮换 locator 扩大重试预算。
- OpenCLI 的 bridge、Chrome、扩展、daemon、CDP、网络、timeout、selector 或未知失败直接返回具体错误，不切换内置浏览器。只有明确的人工作业错误才允许切换。写操作如果返回 `effectMayHaveOccurred`，先读取页面状态确认结果，再决定是否继续。

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

`BROWSER_RUNTIME_UNAVAILABLE` 是内置浏览器兼容性错误码；OpenCLI 失败使用 `OPENCLI_*` 错误码。排查时结合 `browser(action=list_tabs)`、runtime 日志和页签状态判断。

## OpenCLI 流程

OpenCLI 是用户真实 Chrome 的优先浏览器方案。统一 `browser` 工具负责检查和调用它，
Agent 不需要自行拼接 shell 命令或在内置浏览器与 OpenCLI 之间来回切换。

1. 读取当前安装的 `opencli-browser` Skill；以实际 Skill 文档为准，不猜参数。
2. 统一工具内部执行：

   ```bash
   opencli doctor
   ```

   doctor 失败时，统一工具返回 `OPENCLI_*` 错误；除人工操作类错误外，不切换内置浏览器。

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

OpenCLI 的 session/page identity 与内置浏览器的 `browserTabId` 不同，不要交叉传递。
统一工具会把 OpenCLI target 包装成不透明 `browserTabId`；Agent 只需回传上一次工具结果
中的值。遇到登录墙、验证码或人工接管时，工具会创建一次性内置页签并尝试同源 auth
handoff，然后等待用户操作；handoff 使用 120 秒短期预算，导入完成后立即丢弃中转
Cookie 和 storage 值，最多导入当前站点的非 HttpOnly Cookie 与 local/session storage，
不复制密码、完整 Chrome profile、扩展或跨站凭据。

## 安装和运行时

### 内置浏览器引擎

桌面应用安装包不包含 Chrome for Testing。首次启动完成后，应用在后台从 Fusion
受管组件服务预下载 `browser-engine`，过程不打开浏览器窗口、不弹窗，也不唤醒系统
Chrome/Edge。下载、摘要、签名、解压、文件布局和受管 marker 校验全部通过后才会激活
版本；启动阶段不会执行浏览器程序。若受管组件服务不可用，运行时会先探测本机已有的
Chromium 系浏览器；Windows 还会在 iyw-claw 数据目录内下载并校验固定版本的 Chrome
for Testing 作为最后兜底，不依赖安装目录可写或用户当前 Chrome profile。

用户首次打开内置浏览器时，如果后台下载仍在进行，前台请求会等待同一个安装任务，
不会创建第二个下载。离线或未登录时，后台失败保持内部状态；用户主动重试时才显示
可操作的错误。运行时会复用已验证的 last-known-good 引擎，并对失败启动做有限重试；
启动前还会对受管或本机 Chromium 可执行文件执行隐藏启动探针；探针失败时继续尝试
其他候选引擎，首次启动失败也会清除缓存依赖并重新探测，避免把损坏的 marker 固定住。
受管引擎使用 iyw-claw 自有 profile，不绑定正在运行的普通浏览器。

OpenCLI 由 Internet Tools bootstrap 安装到托管 Node prefix。当前版本由 `src-tauri/src/commands/internet_tools/types.rs` 的 `OPENCLI_VERSION` 固定。`src-tauri/src/acp/connection.rs` 把托管 bin 目录加入 Agent 和 ACP terminal 的 PATH；`src-tauri/src/commands/internet_tools.rs` 提供 bin 目录和 `MCPORTER_CONFIG`。安装完成后，OpenCLI 自带的 `opencli-*` Skills 会同步到 central Skill 目录。

Agent 不应向用户暴露托管绝对路径、token、cookies 或完整命令细节；只在工具缺失、doctor 失败或需要人工登录时报告具体前置条件。
