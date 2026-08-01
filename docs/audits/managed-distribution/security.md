# 安全审计报告（security.md）

> Audit A 只读基线 · 2026-08-01 · 三仓提交：iyw-claw b46a4c4 / iyw-fusion-api d0df3bc / skill ea76a4e
> 结论先行：Audit A 确认 1 个 P0 凭据缺陷（IYW-SEC-001）与 2 个供应链/访问控制 P1（IYW-AUTH-001、IYW-SKILL-008 归属完整性）；本轮新增低危管理页暴露与 key 回显建议（IYW-AUTH-002、IYW-SEC-003）。secret 命中值一律不回显，只报路径与类型。

## 1. Secret / 凭据

### 1.1 已确认
- **IYW-SEC-001（P0）**：`iyw-claw/src-tauri/src/system_skills/git.rs:48` 通过 `crate::git_credential::inject_credentials(cmd, "iyw_lq", "<MASKED>", &askpass)` 硬编码 GitLab 凭据；`system_skills/mod.rs:15` 固定 `REPOSITORY_URL`。凭据在源码与历史（commit `3b6e21b`）中均存在，必须移除、轮换并从历史清理。
  - 影响面：任何能读取该文件/安装包的人获得仓库写读凭据。
  - 本轮扫描限定：当前树命中 1 处（上述文件）；历史扫描命中同源提交；测试夹具中的 `ghp_0123456789...` 为假值，不构成缺陷。
- **IYW-AUTH-001（P1）**：skill 可见性只有 `public|private`，`access.go:15-19` `CanRead = active && (public || IsOwner)`，市场查询仅 `visibility = public`；无法表达“同组织所有登录用户可见”。影响 list/detail/files/versions/dependencies/plan/ticket 全部读路径。
- **IYW-SKILL-008（P0）**：下载完整性——响应字节与声明的 `object_sha256` 不是同一字节序列（动态 ZIP 无冻结对象），属于供应链完整性缺陷（详见 reliability.md）。

### 1.2 已检查未发现（负证据）
- Fusion API `internal/application/relay`：日志使用 `RedactedPreview`（512B 上限）+ `imageErrorPreview` 脱敏，测试 `image_edit_fallback_test.go` 断言 base64/secret 不泄漏。
- Fusion API Redis：只存脱敏运行快照（`adapter/redis/redis.go`），无明文密钥。
- 桌面 `git_credential.rs` 的 credential helper 走 DB 查询，不硬编码值（除 IYW-SEC-001 外）。
- `skill` 仓库当前树无凭据命中（除测试中的假值）。

## 2. 身份、授权与多租户

### 2.1 鉴权一致性
- Fusion 用户路由：`router.go` 对 `/skills` 挂 `TokenAuthStandard + RequireUserTokenInfo`；`/v1` 挂 `TokenAuth`；admin 挂 `adminAuth`（`admin.go:67-75`，constant-time 比较）。
- **IYW-AUTH-002（P3，新发现）**：`admin.go:27-44` 的 `/admin/*` 静态页与静态资源不经过 `adminAuth`，仅 `/admin/api` 有鉴权。静态壳页面无数据，属纵深防御缺口；管理 API 仍受 token 保护。
- admin token 缺失时 `LoadAdminToken` 直接报错（`config/admin.go:30-34`），服务不启动，无“空 token 放行”风险。

### 2.2 多租户与侧信道
- 跨组织枚举：`skill_repository_helpers.go` 市场查询只按 `visibility=public` 过滤，未按 org 细分；私有 Skill 只有 owner 可见（无枚举面），但组织共享语义缺失（IYW-AUTH-001）。
- 缓存 key：Fusion 本轮未发现 Redis 侧缓存 Skill 列表（Redis 仅承载脱敏运行快照与 Nacos 心跳），无缓存 key 缺组织维度问题；`agentrelease` 目录读路径直查 MySQL。
- Snowflake ID：后端 `model.go` 使用 `json:"id,string"` 输出字符串；浏览器端 Skill 市场前端以 `string` 传递（`src/lib/skill-market.ts:10`）。前端 `Number()` 命中点均为本地小整数 ID/计数（folderId、attempt、zoom 等），未发现对 Snowflake 的 `Number/parseInt`。

### 2.3 错误侧信道
- 桌面下载错误：`install.rs` 对 retryable 错误区分；错误信息不回显 artifact 内容。
- Fusion 错误响应：`response.Error` 统一格式；TOS 错误经 `tos.go` 封装为 operation-level 日志，不暴露内部 key。

## 3. 制品供应链（Skill / Agent / App）

### 3.1 桌面直连上游（IYW-DIST-001，P1）
- `binary_cache.rs:111` 直连 `github.com/astral-sh/uv/releases/download`；`registry.rs` 直连 opencode 发行版；`runtime_bootstrap.rs:45-48` 硬编码 Node/Git 镜像与官方 URL；`tauri.conf.json` 仍把 `uv/uvx` 与 `resources/runtime` 打入安装包。
- 风险：无统一摘要/签名 gate，镜像源被替换或上游被劫持时无 quarantine。
- 缓解：`runtime_bootstrap.rs` 有 pinned SHA-256 与 15s/600s 超时；`binary_cache.rs` 有 sha256 校验。

### 3.2 应用更新直连（IYW-DIST-002，P2，新发现）
- `update/version.rs:19,24` 仍把 GitHub `releases/latest/download/latest.json` 作为默认更新清单与下载前缀，未收敛到 Version Center。与 `update/release.rs`（走 gateway `app-updates/v1/check`）并存，路径未统一。

### 3.3 ZIP 解包安全（已检查，良好）
- `acp/skill_package.rs`：拒绝加密条目与符号链接、`MAX_ARCHIVE_ENTRIES=1024`、`MAX_FILES=512`、`MAX_EXPANDED_BYTES=50MiB`、`validate_path`（防路径穿越）、`register_path` 防重复路径。本轮未发现 Zip Slip/炸弹缺口。

### 3.4 发布校验（skill 仓库）
- **IYW-SKILL-013（P2，新发现）**：`experts.toml` `bundle.version=0.0.11` 与稳定标签（最高 v0.0.8）不一致，发布校验缺失（Task 04 质量门禁要求 version 与标签一致）。
- `experts.toml` 依赖引用与 `SKILL.md` 存在性本轮未逐一校验（留 Audit B 或 Task 04 契约校验脚本）。

## 4. 会话与配置安全
- 配置指纹基础设施存在（`acp.rs:6841-6914`），但“写入失败禁止启动”的强制 gate 未闭环（IYW-CONFIG-001）。
- 日志脱敏：桌面 `AGENTS.md` 要求日志不记录密码/token/密钥/图片/base64；Fusion relay 有 preview 上限与脱敏函数（正向证据）。
- MCP 工具暴露与实际 bridge readiness：`connection.rs:1306-1307` 注释显示已避免对 pi 注入 feedback/delegation 工具；未发现伪造 readiness 的静态证据。

## 5. 本轮扫描方法（复现命令）
```powershell
# secret 命中（已脱敏输出，仅路径/类型）
rg -n -g '*.rs' -g '*.ts' -g '*.tsx' -g '*.mjs' -g '*.toml' -g '*.json' --glob '!**/experts/skills/**' '(password|passwd|secret|api[_-]?key|access[_-]?token)\s*[:=]\s*["''][^"'']{6,}|ghp_[A-Za-z0-9]{20,}|glpat-[A-Za-z0-9_-]{20,}|https?://[^\s/@]+:[^\s/@]+@' <repo>/src*
# 历史 secret
git -C iyw-claw log --all -S '<redacted>' -- src-tauri/src/system_skills
# 外部 URL 清单
rg -o -g '*.rs' 'https?://[A-Za-z0-9._~:/?#@!$&()*+,;=%-]+' iyw-claw/src-tauri/src | Sort-Object -Unique
```
原始输出：`evidence/01-rust-unwrap-expect.txt`、`evidence/02-desktop-external-urls.txt`、`evidence/03-credential-pattern-masked.txt`。

## 6. 待 Audit B 复核项
- 静态命中人工确认：`unwrap/expect` 集中点（web/mod.rs 锁、terminal/manager.rs、remote_image 边界有长度守卫）本轮已抽查，未发现用户输入直接触发 panic 的路径，但需按文件逐一确认。
- 动态：TOS 403/404/429/5xx、慢流、截断、Range 异常、摘要错；Git/MySQL/TOS 中断；双实例租约。
- 桌面动态证据必须来自远端 CI/测试机（当前机器禁止编译运行）。
