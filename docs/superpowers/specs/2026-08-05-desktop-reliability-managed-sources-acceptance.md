# 桌面可靠性与统一来源验收附录

本附录冻结主设计中的可执行映射、接口和验收矩阵。实现不得用相近名称、隐式
回退或仅检查文件存在来替代这里的条件。

## 1. 组件身份与激活入口

### 1.1 Agent

| 原始名称 | 中文名 | component key | 包类型 | 离线激活入口 |
| --- | --- | --- | --- | --- |
| Claude Code | 远山 | `claude-acp` | npm bundle | `claude-agent-acp` |
| Codex | 星河 | `codex-acp` | npm bundle | `codex-acp` |
| Gemini CLI | 流光 | `gemini` | npm bundle | `gemini --acp --skip-trust` |
| OpenClaw | 开放之爪 | `openclaw-acp` | npm bundle | `openclaw acp` |
| OpenCode | 云舟 | `opencode` | platform archive | `opencode acp` |
| Cline | 逐风 | `cline` | npm bundle | `cline --acp` |
| Hermes Agent | 赫尔墨斯 | `hermes` | Python wheelhouse | `hermes-acp` |
| CodeBuddy | 青岚 | `codebuddy-code` | npm bundle | `codebuddy --acp` |
| Kimi Code | 月白 | `kimi-code` | npm bundle | `kimi acp` |
| Grok CLI | 知微 | `grok-build` | npm bundle | `grok agent stdio` |
| Pi | 墨川 | `pi-acp` + `pi-coding-agent` | npm bundle | `pi-acp`，受管子进程 `pi --mode rpc` |

`component key` 是跨仓稳定键；AgentType、中文名、npm 包名和可执行文件名都不能
代替它。一个 action 必须同时声明 key、版本、平台、架构、包类型和结构化 argv。

### 1.2 工具与运行时

| 名称 | component key | 包类型 | 激活入口 |
| --- | --- | --- | --- |
| OfficeCLI | `officecli` | platform bundle | `officecli` |
| Agent Reach | `agent-reach` | source/platform archive | `agent-reach` |
| OpenCLI | `opencli` | npm bundle | `opencli` |
| Open Computer Use MCP | `open-computer-use` | npm bundle | `open-computer-use` |
| Node.js | `node` | platform archive | `node` |
| npm | `npm` | Node 配套离线包 | `npm` |
| uv/uvx | `uv` | platform archive | `uv`、`uvx` |
| Git | `git` | platform archive | `git` |

激活 recipe 来自客户端编译期 allowlist；Fusion 只选择已知 recipe 和制品，不能
下发任意 shell 字符串。包内实际 entry 与表不符时必须隔离，不能尝试猜测入口。

## 2. 来源顺序与失败语义

每个 action 返回以下三个数组，数组及组内顺序均为契约：

```text
managed[] -> cn_mirror[] -> upstream[]
```

下载器完整耗尽当前数组后才能进入下一数组；禁止跨组竞速、随机洗牌或因为历史速度
跳组。单个候选只有在换票、完整下载、大小/摘要/签名、归档结构都通过后才算成功。

- 短时下载 URL 过期：同一候选刷新票据一次；再次过期后切换同组下一候选。
- DNS、连接、超时、`408/429/5xx`、来源 `404/410/416`、短读和 Range 不一致：
  当前候选失败，切换同组下一候选。
- 大小、SHA-256、签名或归档结构不符：隔离已下载字节并上报来源 ID，再切换同组
  下一候选；不得复用缓存。
- 票据服务返回 `source_unavailable`：切换同组下一候选。
- `plan_expired`、`catalog_stale`：停止 action 并重新 resolve，不做来源回退。
- `audience_denied`、`version_blocked`、`client_incompatible` 或认证 `401/403`：停止
  action 并显示策略/登录错误，不能借回退绕过授权。
- 下载 URL 的 `401/403` 仅在服务端明确标为 `ticket_expired` 时允许刷新一次。

三组全部失败后保留 active 和 last-known-good。Fusion 不可达时，验签且未过期的目录
缓存仍按同一三组规则执行；编译期灾备只补齐目录已知版本的 `cn_mirror` 和
`upstream`，不得改变 component、版本、摘要或 recipe。

安装阶段只消费候选下载器已落盘并验签的离线制品。npm 使用完整 tarball 依赖闭包，
uvx 使用解释器与 wheelhouse，Git/安装脚本只能读取 staging；子进程运行在 cache-only/
offline 模式并禁用外网。缺少依赖返回 `offline_dependency_missing`，不得转为 npm、PyPI、
GitHub、Git clone 或第三方 installer 联网。

## 3. Skill v2 契约与发现

### 3.1 用户端接口

以下均使用 `POST`，所有 Snowflake ID 都是十进制字符串：

| 接口 | 用途 |
| --- | --- |
| `/skills/v2/list` | 分页、筛选和市场卡片 |
| `/skills/v2/detail` | Skill 元数据和当前可用版本 |
| `/skills/v2/versions` | 可见版本与兼容性 |
| `/skills/v2/file-tree` | 指定版本文件树及文件预览元数据 |
| `/skills/v2/download-plan` | 普通 ZIP 下载的 artifact 元数据 |
| `/skills/v2/install-plan` | 依赖拓扑、目标与全部 artifact 元数据 |
| `/skills/v2/download-ticket` | 为 plan 中 artifact 换取短时 URL |

当前 `/skills/artifact-ticket` 只作为新客户端迁移适配器；旧 `/skills/download` 和
`/skills/install-plan` 仅服务旧客户端。v2 覆盖列表、详情、版本、文件树、普通下载和
安装，不允许其中一部分继续使用 fixture 或旧 transport。

旧接口退出必须同时满足：最低支持客户端已使用 v2、连续 30 天旧接口有效流量为零、
v2 下载/安装成功率达到发布门禁、无未过期旧 plan、运维与用户文档已切换；随后先关闭
`skill_legacy_stream_enabled`，观察一个发布周期后才能删除代码。

### 3.2 Agent 发现目录

`~` 表示用户目录；启用私有 Agent storage 后由运行时注入对应环境变量。

| Agent | 全局发布/发现目录 | 项目发现目录 |
| --- | --- | --- |
| 远山 | `$CLAUDE_CONFIG_DIR/skills`，默认 `~/.claude/skills` | `.claude/skills` |
| 星河 | `$CODEX_HOME/skills`，默认 `~/.codex/skills` | `.codex/skills`、`.agents/skills` |
| 流光 | `$GEMINI_CLI_HOME/.gemini/skills`，默认 `~/.gemini/skills` | `.gemini/skills`、`.agents/skills` |
| 开放之爪 | `$OPENCLAW_STATE_DIR/skills`，默认 `~/.openclaw/skills` | `skills` |
| 云舟 | `$XDG_CONFIG_HOME/opencode/skills` | `.agents/skills`、`.opencode/skills` |
| 逐风 | `$CLINE_DIR/skills`，默认 `~/.cline/skills` | `.agents/skills`、`.cline/skills`、`.clinerules/skills`、`.claude/skills` |
| 赫尔墨斯 | `$HERMES_HOME/skills`，默认 `~/.hermes/skills` | 无 |
| 青岚 | `$CODEBUDDY_CONFIG_DIR/skills`，默认 `~/.codebuddy/skills` | `.codebuddy/skills` |
| 月白 | `$KIMI_CODE_HOME/skills`，默认 `~/.kimi-code/skills` | `.kimi-code/skills` |
| 知微 | `$GROK_HOME/skills`，默认 `~/.grok/skills` | `.grok/skills` |
| 墨川 | `~/.pi/agent/skills` | `.pi/skills`、`.agents/skills` |

安装完成需做三级验证：ownership marker/链接或副本正确；`acp_list_agent_skills` 和
`acp_read_agent_skill` 可读；启动该 Agent 的隔离验收会话并调用安装的无副作用探针。
前两级不能代替运行时发现。任一要求目标未发现时返回 `partial/failed` 和具体目录，
不能只记 warning 后显示成功。星河的 `.system` 目录只读，不作为发布目标。

## 4. 图片资产接口

图片资产使用 `POST /image-assets/v1/upload-plan`、`/upload-complete`、`/access-ticket`
和 `/ingest-url`。upload plan 返回字符串 `assetId/uploadId`、限定方法/headers、过期时间、
期望 SHA-256 和 `maxBytes=20971520`；complete 必须由服务端重新读取对象头和内容，嗅探
MIME、计算大小与摘要，不能信任客户端声明。

Fusion 把生成接口的 Base64 或第三方 URL 通过同一 complete/ingest 流程落入受控存储，
对客户端持久化稳定逻辑 URL 和 asset ID，不持久化第三方预签名 URL。实际读取由
`access-ticket` 换取短链；短链 `401/403 + ticket_expired` 时刷新一次。第三方抓取和
桌面代理均限制协议、重定向、私网地址、MIME 和 20 MiB 响应体。

历史 Base64/data URL 在读取时幂等迁移：先解码并执行同样的 20 MiB/MIME/摘要校验，
成功后原子替换为资产 URL；无效或超限内容只允许旧视图本地展示并标记不可重发，不能
写入日志。URL-only 图片必须通过显示、编辑、下载和再次发送四条路径。

## 5. Agent 交互与中文矩阵

所有 Agent 对话框固定显示稳定 UI 模式 `iyw_auto`，中文为“自动模式”。有原生自动
模式时映射原生 ID；没有时由统一适配层映射到该 Agent 已验证的可执行模式与应用权限
策略。UI 仍保持 `iyw_auto`，不得回显另一个模式名；找不到安全映射时阻止启动并显示
中文兼容性错误，不能静默降级。

| Agent | 名称 | 自动模式 | 安装状态 | Skill 发布反馈 |
| --- | --- | --- | --- | --- |
| 星河 | `Codex -> 星河` | 必验 | 全阶段中文 | 目录、成功/部分/失败中文 |
| 远山 | `Claude Code -> 远山` | 必验 | 全阶段中文 | 目录、成功/部分/失败中文 |
| 流光 | `Gemini CLI -> 流光` | 必验 | 全阶段中文 | 同上 |
| 开放之爪 | `OpenClaw -> 开放之爪` | 必验 | 全阶段中文 | 同上 |
| 云舟 | `OpenCode -> 云舟` | 必验 | 全阶段中文 | 同上 |
| 逐风 | `Cline -> 逐风` | 必验 | 全阶段中文 | 同上 |
| 赫尔墨斯 | `Hermes Agent -> 赫尔墨斯` | 必验 | 全阶段中文 | 同上 |
| 青岚 | `CodeBuddy -> 青岚` | 必验 | 全阶段中文 | 同上 |
| 月白 | `Kimi Code -> 月白` | 必验 | 全阶段中文 | 同上 |
| 知微 | `Grok CLI -> 知微` | 必验 | 全阶段中文 | 同上 |
| 墨川 | `Pi -> 墨川` | 必验 | 全阶段中文 | 同上 |

“全阶段”至少包括解析计划、换票、托管下载、国内镜像回退、上游回退、校验、离线
安装、激活、回滚、部分发布和失败。模式名、禁用原因、来源名、错误和修复动作均需
`zh-CN` 与英文资源；验收扫描 UI，不接受只验证翻译 JSON 中存在键。

## 6. 星河 Windows 预置提示词

仅当 `target_os=windows` 且 Agent 为星河时，把以下内容写入新 instruction generation
的 `base_instructions`。它只在系统提示层注入一次，不拼进每条用户消息；非 Windows
和其他 Agent 不包含该段，用户/项目更高优先级指令保持有效。会话创建时持久化
generation，旧会话恢复保持其原代际，不能受全局 catalog 重写影响。

```text
在 Windows 执行 shell 命令时，优先使用 PowerShell 7：
- 首选 `& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command '<script>'`；仅当该文件不存在时，才使用已明确探测到的其他 `pwsh`。
- 外层 shell 为 PowerShell 时，传给 `-Command` 的脚本优先用单引号包住，使 `$p`、`$i`、`$lines` 等变量留给内层 PowerShell 展开。脚本自身需要单引号时，在外层单引号字符串中写成两个单引号。
- 不要在 PowerShell 字符串中使用 Bash 风格的 `\"` 转义双引号；按 PowerShell 规则使用单引号、成对双引号或反引号。
- `rg` pattern 含 `|`、`(`、`)`、`\` 等字符时，把 pattern 作为单引号参数，避免被解析成管道或表达式。
- 避免多层嵌套引号；复杂命令拆成更简单的单个命令执行。

带行号读取文件：
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command '$p="models\file.py"; $lines=Get-Content -Path $p; for($i=1;$i -le [Math]::Min(80,$lines.Count);$i++){ "{0,4}: {1}" -f $i,$lines[$i-1] }'

简单搜索：
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command 'rg -n "simple_text" models configs docs'

含管道或括号的正则搜索：
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -Command 'rg -n ''audible|Audible|sound_prob|pred_sound_prob'' models configs docs'
```

静态验收解析生成的 JSON，确认 Windows 每个星河 model entry 恰好含一次该段，其他
目标为零；连续启动和保存设置后 marker 仍恰好一次。升级前 legacy 会话恢复不出现新
段，新会话采用新 generation；若并发两代会话，两者分别保持其指令。argv round-trip
必须保留 `$p/$i/$lines` 和正则中的 `|`，且提示词示例不含 Bash `\"` 写法。Windows
发布机另执行三个模板，核对退出码和输出。

## 7. Windows 权限边界与启动协议

### 7.1 可执行文件清单

| 文件 | manifest | 完整性与职责 |
| --- | --- | --- |
| `iyw-claw-launcher.exe` | `asInvoker`、`uiAccess=false` | Explorer 启动的中完整性公开入口 |
| `iyw-claw-broker.exe` | `asInvoker`、`uiAccess=false` | launcher 先启动的中完整性 broker |
| `iyw-claw-credential.exe` | `asInvoker`、`uiAccess=false` | 无凭据 RPC shim；可继承 Git 调用方完整性 |
| `iyw-claw.exe` | `requireAdministrator`、`uiAccess=false` | 唯一高完整性桌面主程序 |

launcher 是快捷方式、文件关联、更新后重启和安装完成启动的唯一公开入口。每次入口调用
都执行 `ShellExecuteExW` 的 `runas`，因此每次都会出现 UAC；已有实例也必须先完成 UAC，
再由短命高完整性进程经认证通道要求现有主程序聚焦并退出。主程序不能以普通权限继续，
也不能在没有匹配 broker 时进入工作区。

### 7.2 冷启动顺序

1. launcher 从自身 token 捕获用户 SID、logon SID、Windows session ID、PID/creation
   time，生成 256-bit launch nonce、随机 launch ID 和协议版本。
2. launcher 先以自身 token 创建 broker，并保持 supervisor 进程句柄；broker 绝不由
   管理员主程序创建。
3. broker 创建每次启动唯一的本地 named pipe，设置 `PIPE_REJECT_REMOTE_CLIENTS` 和
   显式 DACL，仅允许发起用户 SID 与 `SYSTEM`，不授予 `Everyone` 或宽泛管理员组。
4. launcher 用 `runas` 启动主程序。若 UAC 使用不同账户，用户 SID/logon SID 不匹配，
   主程序拒绝连接并显示“请以当前管理员用户启动”，不能跨用户使用原工作区数据。
5. broker 对每个连接用 `GetNamedPipeClientProcessId` 反查 token，并校验角色、精确
   PID、creation time、用户 SID、logon SID、session ID、完整性、nonce、协议版本和
   单调 request ID。nonce 不写日志，也不作为唯一授权依据。

禁止 Explorer COM 降权、`CreateProcessWithLogonW`、缓存密码、`uiAccess`、放宽 UIPI
消息过滤或从提升令牌猜测普通令牌。

### 7.3 拖放与凭据

普通 Explorer 文件拖放由 broker 所有的中完整性 drop surface/overlay 实际接收；直接
投向高完整性 Tauri 窗口不作为成功路径。broker 只转发规范化绝对路径、CSS 坐标、窗口
标签和事件阶段；拒绝设备路径、远程客户端、跨用户/跨 session、超量消息和重放。主程序
仍执行符号链接、regular-file/目录、MIME、大小和远程工作区规则校验。

Git 配置只调用 `iyw-claw-credential.exe`。shim 不读取 SQLite 或系统凭据库，不持有
长期 token，不启动 Tauri；它用该 terminal 注册的一次性 capability 向 broker 转发 Git
credential stdin/stdout。broker 校验 SID/session、进程树关联和 capability，terminal、
main 或 launcher 任一结束即撤销。响应、命令行和日志不得包含密码、token 或完整 URL。

### 7.4 生命周期验收

- UAC 取消或 main 启动失败后，不残留 broker、drop surface、pipe 或 capability。
- main 正常退出/崩溃、launcher 或 broker 被终止、更新和卸载均有有序清理；NSIS 停止
  并等待 launcher、broker、credential shim、main 和 MCP，不依赖镜像名全局强杀。
- 同用户同 session 的握手成功；不同用户/RDP session、错误或复用 PID、错误 nonce、
  重放 request、猜 pipe 名和远程 pipe 均被拒绝。
- 最终包读取四个 EXE 的嵌入 manifest，并记录 token integrity、SID、logon SID、
  session ID、parent/peer PID。main 为高完整性；launcher/broker 为中完整性。shim 的
  完整性可继承调用方，但安全能力始终仅限一次 RPC。
- 冷/热启动每次各出现一次 UAC；普通 Explorer 拖放和 Git 凭据交互不追加 UAC。
- main 退出后 pipe 不可连接，terminal/main 结束后的 capability 必须失效。
