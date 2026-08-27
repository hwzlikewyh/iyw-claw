# 内置浏览器与 WebView 内存治理设计

## 背景与证据

2026-08-25、2026-08-26 和 2026-08-27 的三份客户端日志均在打开内置浏览器附近中断，
没有主进程正常退出标记。8 GiB 机器当时最低可用内存约 126 MiB，且有旧的
`agent-browser` / Chrome 进程残留。当前运行实例的进程采样进一步显示：

- `iyw-claw` 主进程树私有内存约 2.31 GiB；
- WebView2 进程私有内存约 1.12 GiB；
- managed `agent-browser` / Chrome 约 699 MiB；
- 一个约 308 MiB 的额外 WebView renderer 与打开 DevTools 的时间一致。

因此，“创建 Chrome 时系统可用内存不足”是本次闪退的高概率直接触发因素。日志没有
Windows dump 或 OOM 终止事件，不能把它表述为已由 dump 证明的唯一根因；本设计同时
补齐启动保护、空闲回收和可观测性，使后续事件可以用同一套数据确认。

当前性能监控仅从 `iyw-claw` 主 PID 向下遍历 descendants。Windows 非提权启动路径会让
managed browser daemon 脱离主进程树，因此约 699 MiB 没有计入性能页。文件工作区在
conversation mode 下只是宽度归零，Monaco 仍然挂载；Automations/Skills 等 route 也只是
覆盖会话工作区。这两类不可见 DOM 会继续持有 renderer 内存。

## 验收标准

1. 创建新 browser runtime 或新标签前，在不影响活动工作的前提下清理已确认的残留进程、
   回收可恢复的空闲 Agent，并重新采样内存；仍为 `shrinking` 或 `emergency` 时拒绝本次
   创建并返回可重试的明确错误。
2. 主窗口隐藏到托盘满 30 秒且不存在活动 Agent、语音或待用户交互时，尝试 suspend 主
   WebView；窗口显示前先 resume。竞态或 API 失败不能造成重新显示后仍处于 suspend。
3. 不可见 30 秒的 Monaco editor 被卸载；文件内容、dirty 状态、打开标签和视图状态保留，
   重新显示时恢复。非会话 route 下的消息 DOM 使用已有 placeholder 路径释放。
4. 性能页把当前 managed browser daemon 及其 Chrome descendants 单列为“内置浏览器”，
   同时计入应用总 CPU、内存和私有内存；不纳入系统 Chrome 或临时 client。
5. 回收逻辑不关闭活动标签、不终止活动 Agent、不打断后台流式回复，不清除浏览器登录
   profile；终端保持现状，本次不修改。

## 非目标

- 不修改浏览器共享 profile、Cookie、缓存和登录态语义。
- 不限制已经运行的 browser runtime，也不因内存压力自动关闭用户正在使用的标签。
- 不改变 Agent 并发上限、用户配置的常驻 Agent 数量或终端生命周期。
- 不把 DevTools 自动关闭作为产品策略；它只作为本次内存诊断中的已知高占用来源。
- 不引入轮询型前端内存采样、自动刷新页面或操作系统级工作集强制裁剪。
- 不处理系统 Chrome、Edge 或其他应用的进程。

## 方案概览

### 1. Browser 启动前置门禁

门禁放在 `BrowserSessionManager::ensure_runtime_running` 的串行启动区间内，位于旧 generation
清理、tab registry 判空之后，`begin_runtime_start` 和实际 `runtime.start` 之前。这样同一
时刻只有一个启动者执行回收与判断，且被拒绝的尝试不会创建新 generation 或留下半启动
状态。

desktop setup 在创建 `BrowserSessionManager` 时传入 `ConnectionManager::clone_ref()` 的浅
引用。它只共享既有 ACP 状态与回收方法，不新建连接集合；`ConnectionManager` 本身不持有
browser manager，因此不形成双向资源所有权。该字段受 `tauri-runtime` 条件编译保护，
server runtime 的 browser stub 和 HTTP 命令不引入桌面内存门禁。

门禁顺序如下：

1. 若 runtime 已经 `Running`，直接复用，不能因为当前内存压力关闭或重启它。
2. `Failed` 状态继续使用既有离线清理和 generation 保护，确保旧资源所有权收敛。
3. 在 profile 加锁前，复用现有 PID、启动时间、可执行文件和 profile 参数校验，清理仅属于
   iyw-claw managed profile 的 stale daemon / engine。
4. 采样 `ResourceSnapshot`。若为 `shrinking` 或 `emergency`，调用现有
   `ConnectionManager::sweep_excess_idle`，每次只回收其策略认定可恢复且无保护原因的空闲
   Agent。活动 turn、pending input、权限/问题、工具、delegation、后台任务、可见租约和
   不可恢复会话均由既有 `reclaim_block_reason` 排除。
5. 回收后重新采样一次系统内存，不用回收前的旧快照做决定。
6. 结果为 `comfortable` 时继续启动；`unknown` 时保守跳过门禁但记录采样不可用；仍为
   `shrinking` 或 `emergency` 时返回 `BROWSER_INSUFFICIENT_MEMORY`，不创建 Chrome。

打开新标签会经过同一保护函数。已有 runtime 下不重复清理 Agent；仅在 `emergency` 时拒绝
创建新标签，避免当前 runtime 已占内存时继续放大压力。`shrinking` 下已有 runtime 的新标签
保持可用，减少正常体验影响。

新错误是可重试资源错误，错误上下文只包含 pressure、available/total bytes 和阈值，不包含
进程命令行或文件路径。前端沿用现有浏览器错误展示和重试入口，不增加弹窗。

### 2. 残留 Browser 进程的安全边界

现有关闭修复的生命周期不变量必须保持：controller 退出后禁止调用
`AgentBrowserCli.run()`、`run_pinned()`、`tab close` 或 `close`。`agent-browser` 会为不存在
的 session 自动创建 daemon，这类“清理”会复活旧 runtime。

启动前清理只接受以下证据同时成立的进程：

- 来自当前 managed profile 的 lock/PID/session 文件，或命令行包含当前 profile 精确路径；
- PID 的当前启动时间与记录一致；
- executable 与已验证 sidecar 或 browser engine 路径一致。

身份不匹配、文件读取失败或权限不足时不得 kill；记录跳过原因并让 profile 获取路径返回
既有结构化错误。清理使用现有 `kill_tree_checked` 和有界 `wait_for_exit`，不扩大到同名系统
Chrome。运行中的 runtime 由 `BrowserRuntime.current` 持有，不能被 stale 扫描命中。

### 3. 主 WebView 隐藏后的回收

Windows 桌面端新增单一 `MainWebviewMemoryController`，由 Tauri app state 托管。它只使用
WebView2 `ICoreWebView2_3::TrySuspend` / `Resume`，不同时设置
`MemoryUsageTargetLevel`。微软文档说明 `TrySuspend` 成功时会自动采用低内存目标；混用两套
策略会增加恢复顺序和状态判断的不确定性。

隐藏流程：

1. 所有主窗口隐藏入口调用统一的 `note_hidden`，递增 generation 并启动 30 秒延迟任务。
2. 延迟到期后重新检查 generation、窗口仍隐藏以及业务 blocker；不能使用隐藏时的旧状态。
3. blocker 包括 prompting/turn in flight、后台 turn、活动工具或 delegation、待权限/问题/
   确认、活动语音以及无法可靠判断的状态。存在任何 blocker 时跳过本次 suspend，不主动中断
   工作；后续业务状态变化不触发高频轮询。
4. 通过 `WebviewWindow::with_webview` 在 WebView 所在线程查询并调用 `TrySuspend`。返回 false
   表示 WebView2 当前不允许 suspend，按正常降级处理。

显示流程在实际 `show` / `set_focus` 之前调用统一 `resume_before_show`：先递增 generation
使所有旧延迟任务失效，再对可能已 suspend 的 WebView 调用 `Resume`。无论 Resume 结果如何
都继续现有显示路径，避免托盘入口永久失效；失败时保留结构化错误，后续显示入口可再次尝试
Resume。重复 resume、重复 hide 和延迟任务迟到均必须幂等。

自动启动即隐藏、关闭到托盘、托盘菜单显示、单实例唤醒以及其他现有 show/hide 入口都接入
同一控制器。窗口真正销毁和应用退出沿用现有生命周期，不为退出流程新增等待。

Windows-only 依赖精确固定为当前已解析兼容组合：Tauri `=2.10.2`、
`webview2-com = "=0.38.2"`。依赖仅进入 `tauri-runtime` / Windows 构建，server runtime 不引用
WebView2 类型。API 不可用、COM cast 失败或回调失败只产生日志，不使应用退出。

### 4. 不可见前台界面的延迟卸载

#### 文件工作区 / Monaco

`FileWorkspacePanel` 接收明确的 `isVisible`，不可见时启动 30 秒 generation-safe 延迟；重新
可见立即取消。延迟到期只卸载 `MonacoEditor` 子树，不卸载工作区 provider，也不关闭文件
tab。受控内容、dirty/save state、tab 元数据继续由现有 store 持有。

卸载前沿用现有 editor dispose 回调清理 action、listener、widget、ResizeObserver 和
decorations。`@monaco-editor/react` 保持默认 `saveViewState=true`，使用其模块级 view state
恢复光标和滚动位置；文件内容仍以 store 为准，不能从 Monaco model 反向覆盖较新的内容。
重新可见时先显示原有 loading 状态，再挂载 editor，布局尺寸不能被延迟状态改变。

编辑器正在 IME composition、保存或文件加载时仍可卸载，因为内容/状态已由现有受控 store
持有；但卸载不得触发保存、丢弃修改或关闭标签。

#### 非会话 route

`WorkspaceContent` 当前在 Automations/Skills 等 route 下保留会话工作区并用 opaque overlay
覆盖。将 route 可见性合并进现有 `ConversationTabView.isVisible`，让消息 DOM 走现有空
placeholder 分支。`ConversationTabView` 组件本身、ACP 连接、事件处理、草稿/队列保护和
`messageScrollPositionRef` 保持挂载；有临时草稿或队列编辑时继续使用既有
`retainHiddenInput` 例外，不能丢输入。

`AuxPanel` 关闭时已经 `return null`，browser detached window 已经 `destroy()`，两者不改
核心语义。终端完全排除。

### 5. 性能监控纳管内置浏览器

性能命令增加可选 browser runtime process snapshot。该快照由 `BrowserRuntime.current` 提供
daemon 的 PID、启动时间和 executable；性能采样线程必须再次用 sysinfo 校验身份。没有
running handle、身份不匹配或进程已退出时，不增加额外 root。

进程范围改为两部分的并集：

- 现有 `iyw-claw` 主 PID 及 descendants；
- 已校验的 managed browser daemon PID 及 descendants。

分类器优先把第二部分统一归入 `group_id = managed-browser-<daemon_pid>`、显示名“内置浏览器”。
daemon role 为 `controller`；Chrome 根据 `--type` 分为 `browser`、`renderer`、`gpu-process`、
`utility` 和 `crashpad-handler`。如果 browser daemon 意外又成为主树 descendant，集合去重，
每个 PID 只计一次。

`memory_used_bytes`、`private_memory_used_bytes` 和 CPU 总量继续从最终 `processes` 求和，因此
新增组会自然进入总计。Agent session 归类仍只消费 Agent 组，不把 browser 误认为 Agent。
性能命令取不到 browser manager 或 server runtime 下没有 desktop runtime 时退回原有主树
结果。

## 并发与失败处理

- 浏览器门禁受现有 `runtime_start_lock` 和 `tab_open_lock` 保护；回收后再次检查 shutdown
  epoch/cancellation，关闭操作优先于迟到启动。
- Agent 回收只调用现有可恢复性策略；回收失败不循环重试，也不扩大候选范围。
- WebView 延迟任务以 generation 拒绝旧任务；显示路径在调用 WebView API 前后都保持可继续，
  不因回收失败阻断用户打开窗口。
- Monaco 延迟卸载以 React effect cleanup/generation 取消，组件或 route 已变化时不提交旧状态。
- 性能采样是只读快照。runtime 在采样中退出时跳过缺失 PID，不把 transient race 当作错误。

## 日志与隐私

关键日志使用现有结构化 target，覆盖：

- browser 启动前/回收后 pressure、available/total bytes、回收 Agent 数和门禁结果；
- stale browser 候选的身份校验结果、清理数量和失败阶段；
- WebView hide generation、blocker、TrySuspend/Resume 结果及耗时；
- 性能采样是否纳入 managed browser、daemon PID 和纳入进程数。

不记录完整命令行、profile 路径、会话内容、URL、用户输入或凭据。热路径不逐进程打印；一次
门禁、状态转换或性能采样异常最多一条汇总日志。

## 验证方式

按仓库规则不新增或运行单元测试、集成测试、端到端测试和快照测试。实现后执行：

1. 沿 start/open-tab -> reclaim -> re-sample -> launch 调用链静态审查所有成功、拒绝、取消和
   清理失败分支，确认拒绝路径没有 generation/进程残留。
2. 沿所有主窗口 hide/show 入口检查 generation、30 秒延迟、blocker、TrySuspend 和 Resume，
   确认窗口重新显示不会卡在 suspend。
3. 静态检查 Monaco dispose/store/IME/save 以及 route placeholder 路径，确认不丢 dirty 内容、
   草稿、队列或后台事件。
4. 用模拟进程关系审查主树与 managed browser 树的并集、去重、身份失配和 runtime 退出竞态。
5. 执行 `cargo fmt --check`、定向 desktop `cargo check`、前端改动文件的 Prettier/ESLint/类型检查、
   `git diff --check`，并核对暂存列表只包含本任务文件。

安装版验证在后续测试制品中进行：分别验证 comfortable 启动、shrinking/emergency 拒绝、活动
Agent 不被回收、托盘隐藏 30 秒后可立即恢复、Monaco dirty 文件恢复，以及性能页的“内置
浏览器”分组与 Windows Task Manager 私有内存量级一致。源码检查不能替代这组 WebView2 和
真实内存压力验证，交付时必须明确保留该风险。
