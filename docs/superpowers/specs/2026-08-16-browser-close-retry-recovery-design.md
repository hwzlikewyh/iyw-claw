# 内置浏览器关闭与重试恢复设计

## 背景

安装版 `0.1.87` 在关闭内置浏览器时出现两个相互放大的故障：主面板无法收起，
随后点击重试也无法稳定恢复。运行日志显示，runtime 的 `close` 命令已经成功，
但后续页签清理仍向已经停止的 controller session 发送 `tab` 命令。
`agent-browser` 会为不存在的 session 自动启动 daemon，因此旧 runtime 被重新创建，
并继续使用共享的 `profile-v1`。

前端 `closeBrowser()` 只有在 `browser_stop_runtime` 成功返回后才设置
`isOpen=false`。后端清理失败因而同时阻止了纯 UI 收起，使用户无法离开错误页面。

本设计修正
`2026-08-16-browser-global-agent-access-design.md` 中“后端成功返回后才把主面板标记为
关闭”的约束。面板可见性属于用户界面状态，不再以后台资源回收成功作为前置条件；
后台仍必须完成有界清理并保留可诊断错误。

## 目标

1. 用户点击关闭按钮或顶部浏览器开关后，主面板立即收起。
2. 关闭流程仍在后台停止页签、runtime 和 detached 浏览器窗口。
3. runtime 停止后，任何补偿清理都不得调用可能自动创建 session 的浏览器命令。
4. 进程和文件兜底清理成功时，关闭整体视为成功，下一次打开可以创建全新 runtime。
5. 清理确实失败时保留结构化错误和可重试所有权，不删除登录 profile。

## 非目标

- 不改变 Agent 浏览器工具、页签访问权限或共享 profile 语义。
- 不删除 Cookie、缓存或登录态。
- 不在本次修复中增加桌面应用单实例机制。
- 不处理系统 Chrome、Edge 或其他应用创建的浏览器进程。

## 前端行为

`BrowserProvider.closeBrowser()` 在通过并发保护后立即设置 `isOpen=false`，随后异步
调用 `browser_stop_runtime`：

1. 第一次关闭立即卸载 docked host 并收起浏览器面板。
2. `closingRef` 继续防止重复 shutdown；顶部开关和工具栏关闭按钮复用同一路径。
3. shutdown 成功时接收最新快照并清除错误。
4. shutdown 失败时保存结构化错误，但不重新打开面板。
5. 用户再次打开时调用现有 `browser_start_runtime`。开始新尝试前清除旧的前端错误，
   最终显示本次 start 的结果。

关闭中的短暂再次点击不启动新 runtime。`toggleBrowser()` 需要把 `closingRef` 对应的
关闭状态视为仍在执行，避免 UI 刚收起后立刻与 shutdown 竞争。实现可继续使用现有
`busy` 状态禁用标题栏按钮，或让 `openBrowser()` 在关闭期间直接返回；不新增可见
对话框。

## 后端资源回收

关闭分为两个阶段，阶段边界必须明确：

### Runtime 存活阶段

- 停止 CDP observer 和帧流。
- drain 页签 registry。
- 可以使用仍存活的 controller session 关闭 target 和页签 session。
- 单个优雅清理失败时继续执行所有页签的兜底清理，不提前返回。

### Runtime 停止阶段

- 停止 runtime controller、sidecar 和匹配 `profile-v1` 的浏览器内核进程。
- 若前一阶段留下待清理页签，只允许执行无副作用的离线清理：
  - 根据已记录 PID、启动时间和可执行文件校验后终止残留页签 daemon；
  - 删除该页签的 PID 和 target 文件；
  - 从 registry 移除对应 handle。
- 禁止调用 `AgentBrowserCli.run()`、`run_pinned()`、`tab close` 或 `close`。这些命令
  在 session 不存在时可能自动启动 daemon。

离线清理发现进程已经退出时视为成功。只有身份匹配的进程仍无法终止，或必要文件
状态无法安全收敛时，才恢复 handle 并返回错误。所有 handle 均应尝试清理，最终返回
第一个错误，同时通过结构化日志记录失败阶段、runtime generation、tab ID 和错误码。

## 状态收敛

- 页签和 runtime 的进程兜底全部成功后，`finish_runtime_stop(None)` 清空 tabs、
  claims、hosts、dialogs、file choosers 和 downloads，并把 runtime 状态恢复为 capability
  状态。
- 仍有资源无法回收时，runtime 状态保持 `failed`，failure code 反映实际兜底错误。
- 下一次 start 只在上一次资源所有权已经清空时创建新 generation；不得复用旧
  controller session。
- 前端轮询继续以 `stateRevision` 拒绝旧快照，关闭后的迟到响应不能重新打开面板。

## 验证

按仓库约定不新增或运行测试文件，实施后执行静态调用链审查：

1. 关闭按钮和顶部开关均先收起面板，再执行同一个 shutdown。
2. 检查 shutdown 的所有错误分支，确认 runtime 停止后没有浏览器 CLI 调用。
3. 检查 handle drain、恢复和成功清除路径，确保不丢失仍需重试的进程所有权。
4. 检查 start/stop 并发保护，确认关闭期间不会启动新 runtime。
5. 检查最终 diff、暂存文件列表和 Git 状态，确保不包含无关改动。

安装版端到端验证不属于本次源码改动的静态完成条件。发布后应在单个桌面主进程下
复现“打开多个页签 -> 关闭 -> 立即重试”，确认日志中旧 controller session 在
runtime close 后不再出现新的 `tab` 命令，且浏览器面板可立即收起并重新打开。
