# 星河升级到 Codex 0.161.0

日期：2026-10-08。

## 目标与边界

内置星河从 0.156.1 升级到 0.161.0，继续使用进程内 App Server 与现有 ACP harness。
升级由应用源码和应用发布承载，系统 npm Codex 不影响内置版本。

验收标准：

1. release tag、tag object、源码提交、生产依赖和 runtime.json 一致。
2. 宿主认证、运行环境、MCP、命令描述和回合归属保留。
3. Windows 同 EXE 沙箱入口及 Unix 执行入口适配新 API。
4. 桌面编译、源码静态审查、协议导出与迁移文件一致性检查通过。

采用生产源码三方迁移：旧上游源码、本地覆盖源码、新上游源码。
保留现有集成边界和需要的本地补丁，使用上游已有的等价后台启动实现。
不扩展星河 UI 以展示 CLI 专有的语音、全屏终端或 Daybreak 控件。

## 来源

- 官方发布：https://github.com/openai/codex/releases/tag/rust-v0.161.0
- tag object：`7e21416b38834816c224ea0dfd135c3de94b2f15`
- source commit：`979011409de0a60b52f179721948e65531d26144`
- 发布时间：2026-10-07 15:58:45 UTC。

## 适配决策

- 同步 21 个 Codex 生产源码覆盖包、依赖清单和主应用 Cargo.lock。
- crossterm patch 改用上游 `efa177859fd9623d57b9fe7ae9bf491ae1ac6ec4`。
- 进程内启动共享 `EmbeddedNetworkPolicy`，环境管理器使用 `ExecServerRuntimeOptions`。
- 宿主 API key 使用策略解析后的独立 serving 认证实例，不改变策略认证账号；凭据与环境不写入全局进程配置。
- 配置重载继续重新应用宿主环境，沿用上游策略认证与现有连接管理器。
- 会话 MCP 状态与目录来自同一已发布代际，支持新的 serverName 过滤且不启动发现连接。
- Windows setup 保留 internal role 参数，并支持上游大 payload 环境传输。
- Linux 在应用初始化前注册上游进程 setup helper；macOS 传递受信的 Codex home 符号链接策略。
- 上游隐藏窗口工厂替代重复的本地 CREATE_NO_WINDOW 代码，保留必要的 Windows HANDLE 转换。
- 命令 description 扩展保留在 Rust item、历史记录、JSON schema 与 TypeScript 导出中。
- 保留 HTTP 413 有界压缩、不可重试客户端错误、600 秒压缩请求预算及明文子代理消息分类。
- 同步上游 SQLite migrations 0056-0058，旧 migration SQL 保持 LF；未启动应用执行数据库迁移。
- 上游移除的历史源码暂留在原位置，不参与当前模块图。

## 验证与限制

按项目 AGENTS.md，本次不新增测试文件，不运行单元、集成或端到端测试。
验证采用桌面/服务器编译检查、版本锁定检查和沿调用链的静态审查。

编译不证明真实账号、MCP 联网、桌面操作或旧会话恢复的端到端行为。
未构建安装包、未替换正在运行的应用；Linux/macOS 原生构建尚未执行。
已通过：

- `cargo check --manifest-path src-tauri/Cargo.toml --bin iyw-claw --locked --offline --jobs 4`。
- `cargo check --manifest-path src-tauri/Cargo.toml --bin iyw-claw-server --no-default-features --features server-runtime --locked --offline --jobs 4`。
- `powershell -NoProfile -File harness/codex/scripts/sync-upstream.ps1 -CheckOnly`。
- `node src-tauri/scripts/prepare-xinghe-worker.mjs`。
- 119 个 Codex crate 版本/提交一致，21 个覆盖包生产依赖按上游解析。
- 稳定版 20 处、实验版 23 处 commandExecution JSON 定义含可选 description；TypeScript 同步。
- 全部上游生产 migration SQL 与本地 LF 内容一致。
- `git diff --check`，无冲突标记或本次新增测试文件。

静态审查覆盖：宿主 key 与策略认证隔离、配置重载、同代际 MCP 查询、
命令描述及迟到完成事件、413/402 错误边界、压缩预算、子代理明文分类、
回合权限、同 EXE Windows setup 和 Unix helper 分流。

编译保留未使用代码、未使用 patch 和 Rust future-compatibility 警告。
未对无关业务代码进行警告清理。
