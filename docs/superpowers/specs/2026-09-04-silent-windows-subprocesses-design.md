# Windows 无控制台子进程设计

## 目标

确保桌面版 `iyw-claw` 在 Windows 上运行任何内置能力时都不弹出可见的
黑色控制台窗口。范围包括 ACP Agent、MCP、npm/Node 工具、音频工具、
插件运行时、版本探测、安装器、更新重启以及当前“打开外部终端”入口。

“无黑框”只针对用户可见的系统控制台窗口；子进程仍可以通过管道把标准
输出和标准错误交给应用记录、展示或参与协议通信。

## 用户体验

- 普通对话、Agent 执行命令、MCP 调用和后台运行时不会出现控制台窗口。
- 语音/音频转写使用 `ffmpeg`/`ffprobe` 时不会出现控制台窗口。
- Agent 检测、插件启动、npm 安装和运行时维护不会出现控制台窗口。
- Windows 下的“打开外部终端”不再调用 `start ... cmd /K` 创建独立窗口；
  命令改为在应用内部/后台执行，并沿用现有输出与错误反馈路径。
- 交互式终端如果仍需要保留，使用已有内置 PTY/终端面板，不启动系统
  `cmd.exe` 窗口。

## 设计

### 1. 统一 Windows 进程启动策略

扩展现有 `src-tauri/src/process.rs` 的标准进程封装，使所有桌面路径上的
`std::process::Command` 和 `tokio::process::Command` 都能统一应用：

- `CREATE_NO_WINDOW`；
- `CREATE_UNICODE_ENVIRONMENT`；
- `STARTF_USESHOWWINDOW` + `SW_HIDE`（仅在需要兼容脚本或第三方运行时
  时使用，避免子进程主动创建窗口）；
- UTF-8 环境与受管 Node/Git PATH 注入保持现状。

调用点不得直接构造未配置的命令。需要明确保留裸 `Command::new` 的代码
必须说明其不是桌面运行路径（例如仅用于 Unix 或独立服务器）。

### 2. 直接调用点迁移

优先迁移已发现的 Windows 桌面调用点：

- `acp/media_tool.rs`：为 `ffmpeg`/`ffprobe` 使用统一隐藏配置；
- `commands/acp.rs`：Pi 版本探测使用统一隐藏配置；
- `commands/computer_use/install.rs`：私有 npm 安装使用统一隐藏配置；
- `acp/terminal_runtime.rs`：直接命令和 Windows shell fallback 均走统一
  隐藏配置；
- `plugin_runtime/process.rs`：插件运行时保留隐藏配置；
- 其他桌面安装、频道和运行时启动点逐一审计，避免只修复 ACP 主链。

对于 Windows `.cmd`/`.bat`：

1. 能解析到真实 Node/JS 入口时，直接运行真实入口；
2. 必须经过脚本时，通过隐藏的 shell 启动，并保持 stdout/stderr 管道；
3. 禁止使用 `start`、`/K` 或其他会创建独立控制台的启动方式。

### 3. 外部终端行为调整

删除 Windows `open_external_terminal_impl` 中的可见终端启动逻辑。命令
执行改为复用应用的隐藏子进程封装，并将结果交给现有任务/终端输出通道。

如果当前 API 只返回“已打开”而不携带输出，则补充后台任务状态和错误反馈，
而不是恢复系统终端窗口。已有 macOS/Linux 行为不在本次范围内，除非共享
代码结构要求同步调整。

### 4. 旧外置进程与 MCP 配置

本次代码只约束由当前桌面应用创建的新进程，不自动终止用户通过其他终端、
旧版外置 Codex 或 DBX 启动的进程。应用启动时继续使用现有 Managed MCP
配置治理，并优先使用内置 Codex worker，避免重新引入 `.cmd` MCP 链路。

诊断日志只记录进程类型、启动方式、PID 和错误摘要，不记录 token、完整
环境变量或完整命令参数中的敏感内容。

## 错误处理

- 子进程启动失败仍返回原有错误类型和可操作提示。
- 隐藏窗口不等于吞掉输出：需要协议通信的 stdout/stderr 继续使用管道；
  无需输出的安装或更新任务使用 `Stdio::null()`。
- `CREATE_NO_WINDOW` 或隐藏启动属性设置失败时，启动直接失败并写入诊断
  日志，不回退到可见控制台。
- 终端任务超时、取消和退出码沿用现有 kill-tree、超时和回收逻辑。

## 验证

### 静态检查

- 扫描桌面 Rust 代码，确认所有 Windows 相关 `Command::new` 调用都经过
  统一隐藏配置，或有明确的非桌面/非 Windows 说明。
- 确认不存在 `start ... cmd /K`、未配置的 `cmd.exe` 桌面启动路径。

### 定向测试

- 为进程封装补充 Windows 条件编译测试，验证标准和 Tokio 命令都设置
  隐藏创建标志。
- 为音频工具、Pi 探测、npm 安装和插件启动保留输出、错误、超时和退出码
  回归测试。
- 验证 `.cmd`/`.bat` 解析到真实入口时参数和工作目录不变。

### 打包冒烟

使用 Release 打包版启动桌面应用，依次执行：

1. 普通 ACP 对话和 Agent 命令；
2. MCP/插件调用；
3. 语音或音频转写；
4. Pi Agent 检测；
5. npm/运行时安装路径；
6. 原“打开外部终端”入口。

每一步同时采样进程树，确认当前 `iyw-claw.exe` 后代中没有可见的
`cmd.exe`、`conhost.exe`、`powershell.exe`、`pwsh.exe` 或其他控制台窗口。
确认内置 worker 仍能完成 ACP 初始化，历史会话和任务输出不受影响。

## 非目标

- 不迁移 Codex 配置目录或修改 Agent 显示名；这些属于独立任务。
- 不清理或终止由用户手工启动、旧版应用或其他开发工具创建的进程。
- 不把系统级全局控制台策略改成注册表或组策略。
- 不改变命令参数、工作目录、权限策略、网络行为或 Agent 协议。
- 不保证第三方程序自身主动调用 `AllocConsole` 或创建 GUI 窗口的行为；
  应用只保证自己的启动链不请求可见控制台。
