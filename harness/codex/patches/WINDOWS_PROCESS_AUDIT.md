# 星河 Windows 后台进程窗口审查

基线：固定上游 `rust-v0.153.4`，提交 `3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`。
审查对象是 `iyw-xinghe-worker` 的 Windows 实际依赖图，以及主程序的 worker 启动边界。
`CREATE_NO_WINDOW` 只作用于非交互式控制台子进程；GUI、UAC 和用户明确打开的终端不应被强行隐藏。

## 入口与覆盖

| 路径 | 正常启动 | 失败或结束分支 |
| --- | --- | --- |
| 主进程启动 worker | 自身 GUI 程序及隐藏进程封装 | 不回退到外置代理 |
| 插件 Git 同步 | `PluginGitMode::command` 设置隐藏标志 | 远程查询、当地查询、clone、fetch、checkout 走同一工厂 |
| 插件手动安装 | Manual 工厂，参数、环境保持原样 | 输出和退出码判断保持原样 |
| npm 插件物化 | `npm_source` 命令设置隐藏标志 | 安装失败输出保持原样 |
| 同步 Git 工具 | 既有 `hidden_command` 工厂 | 不改变超时和输出 |
| 异步 Git 工具 | Job 挂起创建同时保留隐藏标志 | Job 失败只移除挂起标志，保留隐藏标志 |
| MCP stdio | 工厂直接设置隐藏标志 | Job 创建失败、分配失败后的新命令同样隐藏 |
| MCP 动态 HTTP 头 | 头部命令的 shell 设置隐藏标志 | 超时、凭据解析和重试不变 |
| 会话和工具 Hook | 自定义 shell、默认 shell 均在共同工厂隐藏 | Job 回退不清零标志，taskkill 清理也隐藏 |
| 旧 notify Hook | argv 工厂隐藏 | 保留原环境和返回值 |
| 执行服务 stdio | 连接命令工厂隐藏 | taskkill 清理隐藏，保留直接终止回退 |
| 文件沙箱 helper | `fs_sandbox::spawn_command` 隐藏 | stdin/stdout、权限、响应超时和 kill-on-drop 不变 |
| 旧 shell 执行 | `spawn_child_async` 隐藏 | 保留退出码、输出上限、取消、网络环境 |
| Shell 快照 | core 的脚本捕获命令隐藏 | PowerShell/Cmd 原本不支持快照，不归因于其版本探测 |
| 历史文本搜索 | 后台 rg 隐藏 | 找不到 rg 时的原有回退不变 |
| 外部 bearer 认证 | 认证命令隐藏 | 不改变凭据缓存、cwd 和失败处理 |
| AWS 刷新命令 | 上游显式认证刷新命令隐藏 | 不改变认证来源及刷新锁 |
| Code mode 宿主 | 后台宿主进程隐藏 | 管道、进程组和取消不变 |
| PowerShell 探测 | 既有 shell-command 补丁及上游 world-state 标志 | 路径探测、2 秒版本探测超时不变 |
| Windows 沙箱 | 既有非提权 setup、junction、rg、runner 标志 | 不替换受限令牌、私有桌面或 UAC |
| 三个专用 helper | 二进制使用 GUI subsystem，不分配控制台 | 验包阶段拒绝 console subsystem |

## 排除与边界

- `worktree`、CLI、TUI、cloud-tasks 不在当前 worker 的 Windows 依赖图，不把其中源码搜索结果当作本次运行来源。
- exec-server 的 `shell_snapshot` 模块只在 Unix 编译；未添加无效 Windows 分支。
- 上游协议导出使用 prettier 属于代码生成，当前 worker 不调用。
- 应用反馈中的 doctor 仅在显式反馈请求调用；当前 helper 不提供外置 doctor CLI。helper 自身的 GUI subsystem 防止分配控制台，不新增不支持的 doctor 功能。
- OAuth 浏览器、GUI 应用和 UAC 是交互界面，保留其显示行为。
- ConPTY 是应用内交互终端，保留伪控制台属性、Job、输入输出通道。不能把 conhost 存在等同于黑框。
- 用户的 shell 脚本、Hook、MCP 服务仍可显式请求打开新终端或 GUI；本次不劫持其后续任意进程行为。
- 第三方 AWS SDK 的用户自定义 credential_process 不等价于表中上游显式刷新命令，没有将任意第三方 SDK 进程都宣称为已验证。

## 证据与验证

- worker.7 用户复现：捕获真实可见 WindowsTerminal/PseudoConsoleWindow；其 Git 子进程直接属于 `--internal-xinghe-worker`。
- 同一安装 Git 的只读 `rev-parse`、当地 `ls-remote` 在隐藏标志下均 exit0，窗口观察器没有新增终端事件。
- 扩展后的 Windows x64 独立 worker 与 sandbox helper `cargo check --locked --offline --lib --bins` 通过。
- Cargo metadata 核对 9 个新增路径补丁：生产依赖声明的版本范围、feature、optional、target 和默认 feature 全部一致。
- 不创建或运行自动测试；没有在本机编译、打包或启动桌面应用。远端签名产物和实际新建/恢复会话仍需验证。
- 检查通过不等于运行问题已经消失；交付前应记录产物来源、签名、helper subsystem，交付后关联新包版本和窗口事件。
