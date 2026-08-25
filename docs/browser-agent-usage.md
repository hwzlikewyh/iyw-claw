# Agent 浏览器调用与 OpenCLI 回退

## 默认调用链

用户没有指定浏览器时，Agent 使用 iyw-claw 内置托管浏览器：

```text
browser_list_tabs
  -> browser_open / browser_snapshot
  -> browser_click / browser_fill / browser_press / browser_scroll / browser_wait
  -> fresh browser_snapshot
  -> 业务结果验证
```

MCP 工具由 `src-tauri/src/acp/delegation/tool_schema.json` 广告，经内置 HTTP MCP gateway 和 delegation listener 分发到 `BrowserSessionManager::execute_agent_tool`。交互动作最终使用固定页签的 `agent-browser --cdp ... --pin-tab` 控制器，不是 Tauri WebView 的 DOM 事件。

## 内置浏览器引用规则

- `browser_list_tabs` 返回的 `tabs[].browserTabId` 才是其他工具的 `tab_id`。
- `browser_snapshot` 生成的 `@eN` 只对应当时的页面状态。
- 导航、SPA 路由变化、弹窗、列表刷新或一次写操作后，必须重新 snapshot。
- 动作失败时，先判断是 stale reference、selector 问题还是 runtime/session 问题。stale/locator 类问题对同一预期动作总共只有一次恢复机会：重新 snapshot，使用一个新引用或修正后的 locator；不能通过轮换 locator 扩大重试预算。
- runtime/session/daemon/observer 或 timeout 类失败只检查一次当前状态，不要求重新 snapshot；随后由 Agent 决定停止或切换 OpenCLI。写操作如果返回 `effectMayHaveOccurred`，先读取页面状态确认结果，再决定是否继续。

## 错误判断

后端会把控制器结果转换为浏览器错误：

| 错误类别 | 含义 | Agent 行为 |
| --- | --- | --- |
| `BROWSER_SNAPSHOT_STALE` | `@eN` 或页面代际已过期 | 重新 snapshot，换新引用 |
| `BROWSER_INVALID_ARGUMENT` | selector 无效、找不到或不唯一 | 用 snapshot/find 改进定位，不归因于 runtime |
| `BROWSER_TAB_GONE` | pinned tab 已关闭或目标丢失 | 重新列出页签，必要时新建页签 |
| `BROWSER_OPERATION_TIMEOUT` | 控制器动作超时 | 检查页面状态一次，随后停止或切换可用的 OpenCLI |
| `BROWSER_CONTROL_CHANGED` | 点击点被弹窗、横幅或其他元素遮挡 | 先处理遮挡元素或刷新状态，不要归因于 runtime |
| `BROWSER_RUNTIME_UNAVAILABLE` | daemon/session/runtime/observer 可能不可用，或控制器返回未知错误 | 不要把它解释成“点击功能被禁用”；完成一次状态检查后可回退 OpenCLI |

`BROWSER_RUNTIME_UNAVAILABLE` 是兼容性错误码，不等于已证明浏览器内核退出。排查时结合 `browser_list_tabs`、runtime 日志和页签状态判断。后端只向 Agent 返回白名单内的稳定控制器 code，未知 code 统一隐藏。

## OpenCLI 回退流程

OpenCLI 是 Agent 自主选择的外部真实 Chrome 路径，iyw-claw 不会在 Rust MCP 层自动接管。

只有当前 Agent 实际能读取 `opencli-browser` Skill 且能解析 `opencli` 命令时才能进入回退；任一前置条件缺失时应报告具体缺项并停止。

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

OpenCLI 由 Internet Tools bootstrap 安装到托管 Node prefix。当前版本由 `src-tauri/src/commands/internet_tools/types.rs` 的 `OPENCLI_VERSION` 固定。`src-tauri/src/acp/connection.rs` 把托管 bin 目录加入 Agent 和 ACP terminal 的 PATH；`src-tauri/src/commands/internet_tools.rs` 提供 bin 目录和 `MCPORTER_CONFIG`。安装完成后，OpenCLI 自带的 `opencli-*` Skills 会同步到 central Skill 目录。

Agent 不应向用户暴露托管绝对路径、token、cookies 或完整命令细节；只在工具缺失、doctor 失败或需要人工登录时报告具体前置条件。
