# iyw-claw Agent 规则

本文件补充 `AGENTS.md`。发生冲突时，依次遵循当前用户要求、`AGENTS.md`、本文件和仓库文档。

## 工作区职责与边界

当前工作区由三个独立 Git 仓库组成：

- `iyw-claw`：桌面应用与独立服务端，负责技能安装、激活、系统技能更新和客户端体验。
- `iyw-fusion-api`：AI 协议中转与 Skill 市场后端。修改该仓库时，先遵循其 `AGENT.md` 及对应开发计划。
- `skill`：系统技能的版本化发布源。`experts.toml`、各专家目录和发布标签共同定义一个技能版本。

跨仓库改动前先确认接口、数据和发布边界。不要在桌面端复制后端业务逻辑，也不要在桌面端硬编码 Fusion API 的密钥、用户 token 或管理端凭据。

桌面端调用 Skill 市场时，只使用用户端 `/skills/*` 接口与当前会话 `token`；所有 Snowflake ID 必须作为字符串传递、保存和比较。接口字段或错误处理变更必须以 `iyw-fusion-api/docs/skill-market-user-client-integration.md` 和 OpenAPI 为准。

`skill` 是可发布系统技能的权威来源；`src-tauri/experts/skills/` 是桌面应用的内嵌技能基线。涉及内嵌技能、运行时更新或专家清单的改动，必须显式判断是否需要同步两个位置，不得假设任一目录会自动同步。系统技能发布必须满足：

- `experts.toml` 的 `bundle.version` 与稳定 SemVer 标签一致。
- 每个 `[[expert]]` 都有对应的 `<id>/SKILL.md`。
- 依赖只引用已声明的 expert ID，且没有重复、自依赖或循环依赖。
- 已发布的 `v*` 标签不可移动或删除，仓库中不得提交访问令牌或其他凭据。

## 桌面端本地执行限制

禁止在本机编译、打包或启动 iyw-claw 桌面端。不得执行 `pnpm build`、`pnpm tauri`、任何 `tauri:build*` / `tauri:prepare-sidecars` 脚本，也不得在 `src-tauri/` 执行 `cargo build`、`cargo check`、`cargo test` 或 `cargo clippy`。桌面产物由远端 CI 或明确指定的发布环境生成。

除非当前用户明确要求，保持 `AGENTS.md` 的默认交付策略：不运行测试、不新增或修改测试文件。可执行静态审查、`git diff --check`、配置和文档一致性检查；未运行编译或测试时必须如实说明。

## 托管分发与升级保护

- 桌面安装包不得内置系统 Skill、市场 Skill、Node.js、uv/uvx、Git、Agent SDK 或 Agent CLI；这些组件统一由 Fusion API 返回版本决策和短时下载票据，再从 TOS/CDN 下载。
- 桌面端不得把 GitHub、GitLab、npm、PyPI 或其他上游地址作为托管组件的正常下载路径。上游发现、校验、镜像和重试由 Fusion API 的持久化任务中心负责；客户端只允许在明确的灾备策略下使用已编译白名单回退。
- Skill、Agent、CLI 和基础工具必须使用不可变版本目录、校验摘要、原子激活指针与 last-known-good 回滚。发现已安装且摘要、平台和兼容范围均匹配时必须复用，不重复下载。
- 应用更新只能替换 `app` 区域，不得覆盖或删除 `runtime`、`config`、`data`、`logs`、Skill、Agent、CLI、本地库存、用户设置和用户记忆。强制更新只能切换受管版本，不能修改用户拥有的目录。
- Codex 和 Claude Code 每次新建会话前必须通过统一 reconciler 幂等重写受控配置并回读校验；写入失败时禁止带着未知配置启动会话。恢复旧会话不得静默混入新的记忆或策略代际。
- 多智能体协同与实时反馈对新安装默认开启；已有明确用户设置不得被默认值覆盖，后台安全策略保留紧急关闭能力。

跨仓库托管分发改造以 `docs/superpowers/specs/2026-08-01-managed-desktop-distribution-design.md` 和同名计划目录为执行入口。并行任务必须遵守任务包声明的 `scope_write`；SQL、共享 DTO、路由、bootstrap、应用总入口、根配置和 lockfile 由总控集成任务统一修改。

## 代码与产品约定

- 技术架构、条件编译、目录边界、代码风格和服务端环境变量以 `AGENTS.md` 为准，不在本文件重复维护。
- 所有对用户可见的智能体名称必须使用下表中文名。展示层统一通过 `src/lib/types.ts` 的 `AGENT_LABELS` 与 `getAgentDisplayName()` 获取名称，禁止在 UI、日志、提示文字或文档中输出原始英文名。

| 类型键 | 中文名 | 禁止使用的原名 |
| --- | --- | --- |
| `claude_code` | 远山 | Claude Code、claude-code |
| `codex` | 星河 | Codex |
| `open_code` | 云舟 | OpenCode、open-code |
| `gemini` | 流光 | Gemini CLI、gemini |
| `open_claw` | 开放之爪 | OpenClaw、open-claw |
| `cline` | 逐风 | Cline |
| `hermes` | 赫尔墨斯 | Hermes |
| `code_buddy` | 青岚 | Code Buddy、code-buddy |
| `kimi_code` | 月白 | Kimi Code、kimi-code |
| `pi` | 墨川 | Pi |
| `grok` | 知微 | Grok |

- 新增或调整业务流程时，记录足够的脱敏上下文、状态变化、分支决策、外部调用结果和完整错误原因；日志中禁止出现密码、token、密钥、完整个人信息、图片或 Base64 原文。
- 严格保持前端静态导出约束。需要参数化页面时使用查询参数，不新增 Next.js 动态路由。

## 提交与发布

- 开始前检查 Git 状态；只暂存本任务直接修改的文件，绝不夹带或回退已有的用户变更。
- 提交信息使用 `<type>(scope): <中文动词开头的摘要>`，摘要不超过 50 个字符且不加句号。
- `origin` 的 fetch 地址是 GitLab，并配置了 GitLab 和 GitHub 两个 push URL。发布当前分支时使用 `git push origin <branch>`，它必须成功推送到两个地址。
- 推送前先获取远端并确认不存在非快进冲突；推送后分别核验 GitLab 与 GitHub 的结果。推送失败时保留本地提交和工作区，不使用 `reset`、强推或重写历史处理。
