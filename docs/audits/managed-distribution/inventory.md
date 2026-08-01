# 审计基线清单（inventory.md）

> Audit A 只读基线 · 2026-08-01 · 三仓 master/main 当前提交
> iyw-claw `b46a4c4` / iyw-fusion-api `d0df3bc` / skill `ea76a4e`
> 本文件登记仓库、入口、外部依赖与关键数据流，作为审计证据索引的目录。

## 1. 仓库与职责

| 仓库 | 技术栈 | 职责 | 当前分支 |
| --- | --- | --- | --- |
| `iyw-claw` | Rust (Tauri 2 / Axum) + Next.js 16 静态导出 + SQLite (SeaORM) | 桌面/服务端：受信执行、本地库存、下载校验、激活、回滚、会话启动、UI | master（未创建任务分支） |
| `iyw-fusion-api` | Go (Hertz) + MySQL 5.7 (GORM) + Redis + Apollo/Nacos + TOS | AI 协议中转、Skill 市场、App Release Center、Agent Version Center、TOS 适配器、静态管理页 | master（未创建任务分支） |
| `skill` | 目录 + `experts.toml` + SemVer 标签 | 系统 Skill 发布源（非客户端运行目录） | main |

## 2. 入口点

### iyw-claw（Rust，`src-tauri/src/`）
- 桌面入口：`main.rs`、`lib.rs`、`desktop_bootstrap.rs`
- 服务端二进制：`bin_targets/iyw_claw_server.rs`
- MCP 伴生进程：`bin_targets/iyw_claw_mcp.rs`
- HTTP 路由：`web/router.rs`（受保护路由统一 `auth::require_token`；公开端点仅语言设置、下载 ticket、office-watch proxy）
- 命令层：`commands/*`，`_core` 函数供 Tauri/Web 共用
- 会话/ACP：`acp/`（connection、manager、delegation、binary_cache、registry、version_center）
- 消息渠道：`chat_channel/`（manager、command_dispatcher、backends/{wecom,lark,weixin}、webhook、readiness、config_patch）
- 记忆：`user_memory/`（candidate_store/lifecycle、context、launch_context、transaction）
- 系统 Skill：`system_skills/`（git、manager、checkout、activation、manifest）

### iyw-fusion-api（Go）
- 主入口：`main.go` -> `internal/bootstrap/bootstrap.go`
- 路由装配：`internal/adapter/httpserver/router.go`、`admin/admin.go`
- 领域：`internal/domain/{skill,agentrelease,apprelease,relay(protocol)}`
- 应用：`internal/application/{skill,agentrelease,apprelease,relay,admin}`
- 适配器：`internal/adapter/{httpserver,mysql,objectstorage,redis,apollo,nacos,websearch}`
- 迁移：`scripts/mysql/000..030`（031/032/033 为未跟踪草稿，Task 01 未合并）

### skill
- 发布清单：`experts.toml`（bundle.version=0.0.11；稳定标签最高 v0.0.8）
- 各 expert 目录需含 `SKILL.md`（已抽查全部存在，除 `scripts/` 非 expert）
- CI：`.gitlab-ci.yml`

## 3. 外部依赖与来源（Audit A 实测清单）

### iyw-claw 直连上游（与 IYW-DIST-001/002 相关）
| 来源 | 用途 | 位置 |
| --- | --- | --- |
| `github.com/astral-sh/uv/releases/download` | uv 下载 | `acp/binary_cache.rs:111` |
| `github.com/anomalyco/opencode/releases/download/v1.17.13/*` | opencode 发行版 | `acp/registry.rs` |
| `registry.npmmirror.com` | npm/git-for-windows 镜像 | `acp/npm_runtime.rs`、`runtime_bootstrap.rs:45-48` |
| `nodejs.org/dist` | Node 官方 | `runtime_bootstrap.rs:46` |
| `github.com/git-for-windows/git/releases/download` | MinGit | `runtime_bootstrap.rs:48` |
| `github.com/hwzlikewyh/iyw-claw/releases/latest/download/latest.json` | 应用更新清单 | `update/version.rs:19,24` |
| `gateway.iyw.cn/iyw-fusion-api` | Fusion 后端 | `acp/version_center/client.rs`、`update/release.rs:21` 等 |
| `gitlab.iyw.cn/hwz/skill.git` | 系统 Skill Git 源（硬编码凭据） | `system_skills/mod.rs:15`、`git.rs:48` |
| `api.smithery.ai/servers`、`registry.modelcontextprotocol.io` | MCP 服务器目录 | `commands/mcp.rs` |
| `d.officecli.ai/install.*`、`raw.githubusercontent.com/iOfficeAI/OfficeCLI` | OfficeCLI | `commands/office_tools.rs` |
| `github.com/Panniantong/Agent-Reach` | internet tools | `commands/internet_tools.rs` |
| `api.openai.com/auth`、`auth.openai.com` | 登录探测 | `commands/acp.rs` |
| `open.feishu.cn`、`ilinkai.weixin.qq.com` | 渠道 | `chat_channel/backends/{lark,weixin}.rs` |
| `codex-pets.net` | pets 市场 | `pets/marketplace.rs` |

### iyw-fusion-api 外部依赖
- Go modules：hertz、gorm/mysql、redis、agollo、nacos-sdk、ve-tos-golang-sdk、minisign、sonic、semver（`go.mod`）
- 上游 LLM 提供方：仅经配置 `UpstreamConfig`（provider/base URL/API key）运行时选择
- 对象存储：TOS（`internal/adapter/objectstorage/tos.go`、`tos_skill.go`）
- 配置中心：Apollo `iyw.db.common` / `iyw.py.ai`；注册中心 Nacos

## 4. 关键数据流

1. **Skill 安装（当前 broken，IYW-SKILL-001/008）**
   上传（`/skills/uploads/*` 逐文件）→ `direct_upload.go` CompleteUpload 置 ready（raw_size 入 package_size）→ 桌面 `/skills/download` 动态 Deflate ZIP → 客户端按 raw_size 校验长度 → 必失败。
2. **系统 Skill 更新（当前有破坏性 reset 与硬编码凭据，IYW-SKILL-002 / IYW-SEC-001）**
   `latest_stable_tag`（git ls-remote，注入硬编码凭据）→ 比对版本 → `apply_update_locked`（dirty 则 `force_reset` = `git reset --hard`）→ checkout tag → activation。
3. **消息渠道（当前断链，IYW-CHANNEL-001..006）**
   启动 `auto_connect_channels`（每个 enabled 渠道先读 keyring token）→ backend 轮询/webhook → dispatcher → workspace → agent spawn → 回复 → 出站。创建/启用不连接；企微被 token gate 挡在 backend 前。
4. **用户记忆（IYW-MEMORY-001..003）**
   模型主动调用 append/propose → `candidate_lifecycle` 观察/去重/确认 → `transaction` 原子替换。无 TurnComplete 采集闭环；MCP 失败时提示写死 Administrator 路径。
5. **Fusion 中转（relay）**
   入口 `/v1/chat/completions`、`/v1/responses`、`/anthropic/v1/messages` → token 鉴权（仅存在性）→ 快照选择上游 → 协议转换 → 转发 → usage/消费异步记录（有界队列）。外部调用有超时/重试预算；预览 512B 上限 + 脱敏。
6. **版本中心（agentrelease）**
   MySQL 目录/策略 → 快照发布（atomic.Pointer）→ `/agent-platform/*` 查询 → 客户端 Version Center client 决策下载。
7. **管理端**
   `/admin/api`（admin-token 鉴权）→ skill/apprelease/agentplatform 管理 handler；`/admin/*` 静态页无鉴权（壳页面，IYW-AUTH-002 低危）。

## 5. 关键共享资源（Task 13 唯一 owner）
- SQL：`scripts/mysql/*`、共享 API schema/DTO
- router/bootstrap/lib.rs/应用总入口/根配置/CI/lockfile
- feature flag 接线、JobEnqueuer 装配、TOS client 共享适配
