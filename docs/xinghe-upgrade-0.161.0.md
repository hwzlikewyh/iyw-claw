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

## 2026-10-09：补齐线程历史迁移和回合结束语义

供应商中文名任务的日志暴露了升级时遗漏的
`0007_thread_item_lifecycle_timestamps.sql`。官方锁定源码已有该文件，
但本地覆盖包仅包含 0001–0006，导致历史读写报
`table thread_items has no column named started_at_ms`，回合结束后的完整输出
核验也失败。此前迁移内容核对未覆盖缺失文件，不能证明文件集合完整。

修复原样补回官方迁移，为 `thread_items` 增加可空 `started_at_ms` 和
`completed_at_ms`，继续通过 SQLx 的既有迁移、校验和与事务机制执行。
`codex-state/build.rs` 追踪全部六个迁移目录，确保新增文件触发增量重编译。
升级核对同时比较文件集合和 SQL 内容；本次六套共 73 个迁移均与锁定上游一致。

此次任务提前停止的直接原因仍是模型输出了 `final_answer` 后正常 `end_turn`。
公共执行指令明确工具发现、文件检查和下一步说明属于中间进度；有授权且可执行的
工作应继续，最终回复交付结果或说明实际完成范围和具体阻塞。分析、方案、预览、
暂停和取消的边界保留。该指令沿现有配置路径覆盖新建和恢复会话。

中英文 `processCompleted` 文案改为“本轮回复已结束”，保留耗时参数、工具数和
错误分支，避免把回合结束等同于业务任务完成。此修复没有引入自动续跑或任务状态推断。

定向验证：只读打开故障机历史库，并通过 SQLite backup API 复制至内存后执行迁移。
两个字段添加成功，原有 20,702 条历史项、625 个回合和 282 条投影状态记录均保留，
相关字段读取通过；真实运行库未被修改。JSON 解析、参数核对、Prettier 和新增
构建脚本 rustfmt 检查通过。按仓库规则未新增或运行测试文件。

桌面 `cargo check --manifest-path src-tauri/Cargo.toml --bin iyw-claw --locked
--offline --jobs 4` 和服务器 `cargo check --manifest-path src-tauri/Cargo.toml
--bin iyw-claw-server --no-default-features --features server-runtime --locked
--offline --jobs 4` 均通过。新增构建脚本后桌面再次检查通过，编译依赖确认包含
0007，构建输出确认追踪六个迁移目录。现有未使用代码等警告保留。

本次交付为源码修复；未替换安装中的应用，未执行真实账号模型任务或桌面升级。
新版本运行时才会通过既有迁移机制修复历史库，模型行为改善仍需真实任务观察。
