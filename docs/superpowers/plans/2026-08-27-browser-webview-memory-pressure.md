# 内置浏览器与 WebView 内存治理实施计划

**目标：** 在不打断活动浏览器、Agent、语音、后台流式回复和终端的前提下，为内置浏览器
增加启动内存门禁，回收隐藏 WebView/Monaco 的可释放资源，并让性能页统计 managed browser。

**实现边界：** 复用现有 `resource_governor`、browser generation/进程身份校验、
`ConversationTabView.isVisible` 和 Monaco 受控 store。新增模块只承载单一策略；大文件只做
装配。按仓库规则不新增或运行测试文件，使用格式化、定向编译/类型检查和静态调用链审查。

---

## Task 1：浏览器启动和新标签内存门禁

**文件：**

- 新增 `src-tauri/src/browser/resource_gate.rs`
- 修改 `src-tauri/src/browser/mod.rs`
- 修改 `src-tauri/src/browser/manager.rs`
- 修改 `src-tauri/src/browser/manager_runtime.rs`
- 修改 `src-tauri/src/browser/tab_actions.rs`
- 修改 `src-tauri/src/browser/profile.rs`
- 修改 `src-tauri/src/browser/runtime.rs`
- 修改 `src-tauri/src/browser/error.rs`
- 修改 `src-tauri/src/lib.rs`

- [x] 在 desktop `BrowserSessionManager` 中保存 `ConnectionManager::clone_ref()`；server stub 不
  引入该字段。确认 `DelegationInjection` 不持有 browser manager，避免新增 `Arc` 环。
- [x] 在 `profile.rs` 提取“存在 lock 才回收”的 preflight：只复用 PID、启动时间、sidecar/
  engine executable 和 profile 参数校验；controller 退出后不调用任何 session CLI。
- [x] 在 `resource_gate.rs` 封装 runtime-start/new-tab 两种判断。runtime start 先清 stale，采样
  内存，压力下调用一次 `sweep_excess_idle`，重新采样；仍为 shrinking/emergency 时返回可重试
  `BROWSER_INSUFFICIENT_MEMORY`。`Unknown` 降级放行并记录。
- [x] 门禁在 `runtime_start_lock` 内、`begin_runtime_start` 前执行，完成后重新检查 shutdown
  epoch/cancellation。已有 running runtime 直接复用，不触发 Agent 回收。
- [x] 新标签在统一 `create_browser_tab_with_id_unlocked` 路径、reserve/launch 前检查；仅
  emergency 拒绝，覆盖 UI、initial tab 和 Agent `browser_open`，不影响导航已有标签。
- [x] 错误 context 只增加 pressure、available/total bytes 和阈值；不记录命令行/profile 路径。
- [x] 静态审查 Failed cleanup、空 tab registry、start/stop 竞争、Agent cancellation 和失败回滚。

## Task 2：性能页纳入 managed browser 进程树

**文件：**

- 修改 `src-tauri/src/browser/runtime.rs`
- 修改 `src-tauri/src/browser/manager_runtime.rs`
- 修改 `src-tauri/src/commands/performance.rs`
- 修改 `src-tauri/src/commands/performance_processes.rs`
- 修改 `src-tauri/src/commands/performance_windows.rs`（仅在确需共享身份校验时）
- 修改 `src-tauri/src/web/handlers/performance.rs`
- 修改 `src-tauri/src/app_state.rs`（仅核对现有 browser manager 注入，不扩 contract）
- 修改 `src/components/settings/performance-process-groups.tsx`
- 修改 `src/components/settings/performance-process-group-section.tsx`

- [x] 从 running `RuntimeHandle` 只读导出 daemon PID、启动时间和 executable 快照；不暴露
  controller session、CDP URL 或 profile。
- [x] 性能采样刷新 sysinfo 后再次校验 PID 启动时间/executable；通过才把 daemon 及
  descendants 与主进程树做集合并集，身份失配或 runtime 中途退出时退回原结果。
- [x] 分类器优先把额外树归入 `managed-browser-<pid>` / “内置浏览器”，标注 controller、
  browser、renderer、GPU、utility、crashpad role；集合去重确保 PID 只累计一次。
- [x] Tauri 命令传入 managed browser 快照；HTTP/server 路径保持无额外 root，避免改变
  `AppState` 的共享接口。
- [x] 前端分组排序把内置浏览器放在 WebView2 后、Agent 前，并补 `controller` 中文角色；不加
  新卡片或说明文案。
- [x] 静态核对总 CPU、working set、private commit 均从最终 processes 求和，Agent session
  聚合不会吞入 browser 组。

## Task 3：隐藏主 WebView 的 generation-safe suspend/resume

**文件：**

- 新增 `src-tauri/src/webview_memory.rs`
- 修改 `src-tauri/src/lib.rs`
- 修改 `src-tauri/src/commands/desktop.rs`
- 修改 `src-tauri/src/commands/windows.rs`
- 修改 `src-tauri/src/commands/realtime_voice/state.rs`
- 修改 `src-tauri/Cargo.toml`
- 修改 `src-tauri/Cargo.lock`

- [x] Windows/tauri-runtime 下精确固定 `tauri = "=2.10.2"`，增加
  `webview2-com = "=0.38.2"` 和 QueryInterface 支撑 `windows-core = "=0.61.2"`；server
  runtime 不引用 COM 类型。
- [x] `MainWebviewMemoryController` 保存 generation 和 suspended 状态。hide 递增 generation，
  30 秒后重新检查窗口可见性、Agent blocker 和主窗口语音 session。
- [x] `ConnectionManager::has_active_agent_operations` 作为保守 blocker；锁竞争或无法读取视为
  busy。`RealtimeVoiceState` 只增加按 window label 的只读 `has_session`。
- [x] 使用 `WebviewWindow::with_webview` 获取 WebView2 controller，再 cast 到
  `ICoreWebView2_3`。`TrySuspend` callback 只在 generation 未变化且窗口仍隐藏时提交状态；
  false/API 不支持/COM 错误均降级记录。
- [x] `show_main_window`/deferred show 在 unminimize/show/focus 前先取消旧 generation 并调用
  Resume；Resume 失败仍执行 show，后续 show 可再次尝试。
- [x] 关闭到托盘、记忆的 tray close、autostart hidden 均走统一 hide helper。真实退出的直接
  hide 不安排 suspend，避免与 shutdown 争用。
- [x] 检查重复 hide/show、30 秒内恢复、TrySuspend callback 迟到、app exit 和非 Windows 编译。

## Task 4：不可见前台重界面延迟卸载

**文件：**

- 新增 `src/hooks/use-delayed-presence.ts`
- 修改 `src/app/workspace/layout.tsx`
- 修改 `src/components/files/file-workspace-panel.tsx`
- 修改 `src/components/conversations/conversation-detail-panel.tsx`

- [ ] 实现 generation-safe `useDelayedPresence(visible, 30_000)`：可见立即 true，不可见延迟
  false，effect cleanup 取消旧 timer；不使用 viewport 字体或布局变化。
- [ ] desktop `WorkspaceContent` 把 route/mode 可见性传给 `FileWorkspacePanel`。mobile 当前通过
  条件分支自然卸载，不额外保活隐藏 Monaco。
- [ ] `FileWorkspacePanel` 只根据 delayed presence 卸载 `MonacoEditor` 子树；provider、tabs、
  dirty/save/loading state 留在现有 store。保持 `saveViewState` 默认值和现有 dispose 清理。
- [ ] `ConversationDetailPanel` 合并 `isConversations` 到 group visibility，使 route overlay 下的
  消息 DOM 走已有 placeholder；保留组件、ACP 事件、草稿/队列保护及隐藏输入例外。
- [ ] 静态审查 IME composition、保存中、loading、dirty tab、route 往返、分屏/平铺和浏览器
  面板切换；终端文件无 diff。

## Task 5：统一验证和提交审计

- [ ] 执行 `cargo fmt --check`。
- [ ] 执行 desktop 定向 `cargo check --lib --features tauri-runtime`；若环境阻塞，记录完整原因，
  不把静态检查表述为编译通过。
- [ ] 对前端改动执行 Prettier check、定向 ESLint 和 `pnpm exec tsc --noEmit`；命令卡住时改用
  仓库本地 binary 并如实报告。
- [ ] 执行 `git diff --check`，检查每个新增函数 <= 50 行、新文件 <= 300 行、嵌套/日志敏感
  字段和 cfg 边界。
- [ ] 沿 browser start/new-tab、WebView hide/show、Monaco/route visibility、performance 两棵
  进程树四条调用链进行第二轮静态审查。
- [ ] 确认 `git status` 中不包含终端改动、原主 worktree 改动或 `.codegraph` 临时索引；精确
  stage 本任务文件并分层提交，不 push。
- [ ] 明确剩余运行时验证：真实 OOM 压力、WebView2 suspend/resume、安装版托盘恢复和 Task
  Manager 数值对照必须在测试制品中验证。
