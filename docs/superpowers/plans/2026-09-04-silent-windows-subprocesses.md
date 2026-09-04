# Windows 无控制台子进程实施计划

## 目标

在不改变 Agent 协议、参数、工作目录和现有输出管道的前提下，消除桌面
版 Windows 子进程的可见控制台窗口，并让 Hermes 设置入口不再调用外部
`cmd /K` 窗口。

## 步骤

### 1. 加固统一进程封装

**改动**

- 在 `src-tauri/src/process.rs` 中让 std/Tokio 命令统一设置
  `CREATE_NO_WINDOW`。Rust stable 当前不提供可用的 `SW_HIDE` Command API，
  不引入 nightly 或自定义 CreateProcess 封装。
- 保留现有 UTF-8 环境和受管 Node/Git PATH 行为。
- 增加最小的 Windows 条件测试或可审计的 helper 测试。

**验证**

- `cargo fmt --check`。
- 运行进程封装相关 Rust 测试。
- 静态确认两个 helper 都设置隐藏策略。

### 2. 迁移未统一的桌面启动点

**改动**

- `acp/media_tool.rs` 改用统一 Tokio 命令。
- `commands/acp.rs` 的 Pi 版本探测改用统一 std 命令。
- 审计 computer-use、WeCom、插件和浏览器启动路径，修复遗漏的直接
  `Command::new`，不重复改已经使用隐藏 helper 的代码。
- 对 npm `.cmd`/`.bat` 保持现有真实 Node 入口解析；不能解析时仍通过
  隐藏 shell 启动。

**验证**

- 定向 Rust 测试，覆盖音频工具、Pi 探测和运行时启动的输出、错误、超时
  与退出码。
- 静态扫描桌面路径中的裸 `Command::new`。

### 3. 移除 Windows 外部终端弹窗

**改动**

- 删除 `open_external_terminal_impl` 中 `start "" cmd /K` 的 Windows 分支。
- 保留现有前端的失败回退：当本地桌面不能提供内置交互终端时，复制命令
  到剪贴板并提示用户，而不是创建系统窗口。
- 如果共享 API 需要执行后台命令，使用统一隐藏 helper 并接入现有输出
  或错误通道；不改变 macOS/Linux 分支。

**验证**

- 检查 Windows 构建不再包含 `start`、`/K` 的外部终端启动路径。
- 验证 Hermes 设置失败回退仍能复制命令。

### 4. 回归与打包冒烟

**改动**

- 不修改用户配置、旧外置进程或 Agent 显示名。
- 保留本次调试产生的诊断范围，不记录 secrets。

**验证**

- 执行项目规定的 Rust/前端定向检查和构建。
- 用 Release 桌面包依次测试 ACP 对话、MCP/插件、音频、Pi 检测、npm
  安装和 Hermes 设置入口。
- 采样主进程后代，确认没有由当前应用创建的可见控制台窗口。
- 检查内置 Codex worker 初始化、会话历史和任务输出。

## 风险与回滚

- 交互式 Hermes 设置不再自动打开系统终端；失败回退复制命令，用户可在
  自己选择的终端中执行。
- 如果某个第三方程序主动调用 `AllocConsole`，统一启动标志无法阻止其
  自身行为；记录进程信息后单独处理。
- 所有代码改动保持局部，可按文件回滚；不触碰用户数据目录。
