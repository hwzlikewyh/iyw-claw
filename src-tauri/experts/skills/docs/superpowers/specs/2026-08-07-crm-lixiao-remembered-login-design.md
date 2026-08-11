# CRM 与励销记住账号密码及单次自动登录

## 2026-08-11 实测修订

实测环境没有形成可复用的完整凭据：已安装的 `iyw-crm-workflows` 仍是只保存用户名和
Cookie 的旧实现，本机不存在 CRM `session.json`；励销凭据文件只保留应用 Token，
没有 phone、password、业务 Token 或 Cookie。因此会话隔天失效后无法自动重新登录。
仓库实现与当前安装副本的文件哈希也不一致，发布完成不等于已安装版本已同步。

本次修订采用以下约束：

- 继续按本文既有方案，在当前用户的 `~/.iyw-claw/` 下明文持久化账号、密码和会话。
- 只有密码登录接口明确返回“账号或密码错误”时，才删除已保存密码。验证码、TTOCR、
  SSO 回调、应用 Token、业务 Token、网络、超时、限流及其他上游异常均保留密码。
- 为明确区分错误来源，密码登录拒绝使用专用错误类型或等价的明确错误码；不得继续用
  覆盖整条登录链路的通用 `AuthenticationError` 作为删除密码的依据。
- 登录成功的验收必须同时检查 `auth status` 中 `has_saved_account` 和
  `has_saved_credentials` 均为 true，不能只凭接口登录成功或已有 Token 判断完成。
- Skill 更新的验收必须核对仓库版本与实际安装副本；只发布仓库而未同步安装目录不算
  实测修复完成。
- 本机没有凭据时，只允许向当前内部操作人收集一次账号和密码并立即持久化。Agent
  不得联系客户、建议再次向客户索要，也不得把会话过期解释为需要客户重新提供凭据。

修订后的定向测试需覆盖：密码明确错误会清理密码；验证码、SSO、Token 和网络异常均
保留密码；新进程能够从同一配置目录读取完整凭据；安装副本包含新版持久化与错误分类
逻辑。测试只使用隔离配置目录和虚构凭据，不读取或输出真实凭据。

## 背景

`iyw-crm-workflows` 当前只在 `~/.iyw-claw/iyw-crm-workflows/session.json`
保存用户名和 Cookie，Cookie 失效后必须再次询问密码。`lixiao-workflows` 已在
`~/.iyw-claw/credentials.json` 明文保存账号、密码和会话，但自动登录只覆盖部分
API 入口，失败后的凭据失效处理也不统一。

两套 Skill 需要采用一致的用户体验：成功登录后记住账号密码；以后会话失效时只用
保存的凭据自动登录一次；自动登录仍然失败时停止重试。只有本机确实缺少完整凭据时，
Agent 才向当前内部操作人补问一次，禁止联系客户重新索要。

## 目标

- 沿用两套 Skill 的现有 JSON 文件和目录，不引入 Windows 专属能力或第三方依赖。
- 以明文保存账号和密码，保持 Windows、Linux 和 macOS 行为一致。
- 会话失效后，每条 CLI 命令最多自动提交一次保存的账号密码。
- 自动登录成功后，读取类请求最多重放一次。
- 密码登录接口明确拒绝凭据后删除失效密码和用户会话，保留账号供 Agent 继续提问。
- 保持 dry-run 不读取本地会话或凭据、不访问网络的现有保证。
- 所有 CLI 输出、日志和文档示例都不得回显真实账号、密码、Cookie 或 Token。

## 非目标

- 不加密本地账号密码，也不接入 DPAPI、Keychain、Secret Service 或系统 keyring。
- 不改变 CRM 或励销的上游登录协议、验证码流程和业务 API contract。
- 不让两套 Skill 共用同一个新凭据文件，也不迁移励销已有的凭据文件路径。
- 不自动重放解锁、扣减额度或其他可能产生副作用的励销操作。

## 存储设计

### IYW CRM

继续使用 `~/.iyw-claw/iyw-crm-workflows/session.json`。允许字段调整为：

- `version`
- `username`
- `password`
- `cookies`

显式账号密码登录只有在完整登录和首页验证成功后，才同时保存账号、密码和最新
Cookie。`auth status` 保留 `has_username`，并增加 `has_saved_account` 和
`has_saved_credentials` 布尔值，不输出账号或密码。前者表示用户名存在，后者表示
用户名和密码都存在。

CRM 密码提交响应明确显示账号或密码错误时，删除 `password` 和 `cookies`，保留
`username`。登录页 Token 缺失、首页验证失败及其他认证链路异常均保留密码。
`auth logout` 删除整个 `session.json`。

### 励销

继续使用 `~/.iyw-claw/credentials.json`。现有 `phone` 和 `password` 字段保持可读，
因此无需数据迁移。`password-login` 明确成功后立即保存账号和密码，避免后续 CRM SSO
或 app-session 故障导致再次索要；Cookie、SSO Token 和业务会话仍在各自阶段成功后更新。

励销 `password-login` 接口明确拒绝账号或密码时，删除 `password`、`cookies`、
`access_token`、`business_token`、`refresh_token` 和对应用户会话元数据，保留
`phone`、应用级 `app_token` 以及独立的 TTOCR/IYW Token。其余登录阶段即使返回通用
认证错误也保留密码。`auth status` 增加
`has_saved_account` 和 `has_saved_credentials`；前者只检查 phone，后者同时检查 phone
和 password。暂时保留已有 `has_account`，并让它继续作为完整凭据状态的兼容别名。
`auth logout` 继续删除整个 `credentials.json`。

两套存储继续使用同目录临时文件加 `os.replace` 原子落盘。文件创建后在支持 POSIX
权限的平台尽力设置为当前用户读写；由于用户明确选择跨平台明文 JSON，不增加
Windows 专属 ACL 逻辑。共享同一个 `config-dir` 的认证和写入操作必须串行；需要并行
处理业务数据时，先由主 Agent 完成认证，再让子任务只读复用会话，或为测试使用独立
`config-dir`，避免重复登录和最后写入覆盖。

## 单次自动登录状态机

每条 CLI 命令维护进程内的 `reauth_attempted` 状态，初始为 `false`：

1. dry-run 直接生成计划结果，不加载凭据、不验证会话、不访问网络。
2. 真实请求先使用当前 Cookie 或 Token。
3. 只有请求明确抛出 `AuthenticationError` 时，才进入自动登录分支。
4. 如果本命令已经自动登录过，直接返回 `authentication_required`。
5. 如果缺少保存的账号或密码，直接返回 `authentication_required`。
6. 将 `reauth_attempted` 置为 `true`，使用保存的账号密码登录一次。
7. 登录成功后保存新会话；读取类原请求重放一次。
8. 密码登录接口明确抛出 `CredentialRejectedError` 时清理失效密码和用户会话，再返回
   `authentication_required`，不进行第二次登录。其他 `AuthenticationError` 保留密码。
9. 超时、断网、限流、验证码服务失败和其他非认证错误原样返回；不删除保存密码，
   也不把它们转换成要求用户重新输入密码的错误。

Agent 收到 `authentication_required` 后，先检查非敏感状态。账号仍存在但密码确实缺失
时，只向当前内部操作人询问密码，并使用仅传 `--password` 的登录形式安全复用本地保存
的账号；CLI 不在状态输出中暴露账号。账号也不存在时，向当前内部操作人分两次依次询问
账号和密码。取得新凭据并登录成功后立即恢复原任务。任何分支都不得联系客户索要凭据。

## CRM 命令行为

- `auth login`：`--username` 与 `--password` 同时提供时使用新账号登录；只提供
  `--password` 时复用本地保存的用户名；成功后保存账号、密码和 Cookie，失败时不保存
  本次输入。缺少可复用用户名时返回 `invalid_input`。
- `auth ensure`：先验证 Cookie；会话失效后按状态机自动登录一次，成功时返回
  `status: reauthenticated`。
- `api customer-search`：先使用现有会话查询；若响应表明会话失效，则自动登录一次并
  将同一查询重放一次。客户查询是读取操作，可以安全重放。
- `auth status`：只输出路径、配置状态、Cookie 数量、是否保存账号以及是否保存完整
  凭据。
- `auth logout`：删除包含账号、密码和 Cookie 的整个 CRM 会话文件。

自动登录与请求重放放在 CLI 编排层，`CrmClient.login()` 继续只负责一次真实登录，
避免客户端内部递归重试。

## 励销命令行为

- `auth login`：`--phone` 与 `--password` 同时提供时使用新账号登录；只提供
  `--password` 时复用本地保存的 phone；保留现有验证码和 SSO 链路，完整成功后才保存
  账号密码。缺少可复用 phone 时返回 `invalid_input`。
- `auth ensure`：只捕获 `AuthenticationError` 触发一次自动登录。普通业务错误和可重试
  上游错误不再被误判为会话过期。
- 读取类 `api` 命令：保留认证失败后自动登录并重放一次的能力，改为共用统一的一次性
  状态，并只在密码登录接口明确拒绝时清理失效凭据。
- 有副作用的 `api` 命令：执行前完成会话预检；请求发出后不因认证异常自动重放。
  首批明确包含 `company-unlock`，后续新增写操作必须显式加入非重放集合。
- `workflow` 命令：真实执行前统一完成一次会话预检。这样过期会话会在工作流开始前
  自动恢复，不需要把整个多步骤工作流重新执行。工作流中途出现认证错误时停止并返回，
  避免重复解锁或重复消耗额度。
- `api list`、`auth status` 和所有 dry-run 不触发自动登录。

## 错误和隐私

- `authentication_required` 表示没有保存的完整凭据、密码登录接口明确拒绝保存凭据，
  或同一命令已经用保存凭据尝试过一次。后两种情况必须通过非敏感状态区分。
- `upstream_unavailable`、限流和验证码求解错误保留原错误码及 `retryable` 属性。
- 清理失效凭据是定向字段删除，不得用 `None` 更新后意外保留旧密码。
- `public_data`、CLI 成功输出、CLI 错误输出和 `auth status` 均不得包含真实账号或密码。
- 文档只使用 `<CRM账号>`、`<CRM密码>`、`<励销账号>` 和 `<励销密码>` 占位符。

## 文档与 Agent 指引

同步更新两套 Skill 的 `SKILL.md`、`agents/openai.yaml` 和
`references/commands.md`：

- 默认先复用会话，失效时自动提交保存凭据一次。
- 只有状态确认本机缺少完整凭据时才使用 `ask_user_question`，并且只询问当前内部操作人。
- 保存了账号但密码已清理时，只问密码；两者都缺少时仍严格一次只问一个字段。
- 禁止联系客户、建议找客户或要求客户重新提供账号密码。
- 不把网络或验证码错误解释为密码错误。
- 明确 `auth logout` 会同时移除保存的账号密码和会话。

## 测试与验收

### 存储测试

- CRM 能保存和加载 `password`，状态输出只显示 `has_saved_account` 和
  `has_saved_credentials` 等非敏感布尔值。
- CRM 定向失效清理保留用户名，删除密码和 Cookie。
- 励销现有含明文 `phone/password` 的文件无需迁移即可读取。
- 励销状态能区分只保存 phone 和保存完整 phone/password。
- 励销定向失效清理保留 phone、app token 和 TTOCR/IYW Token，删除密码及用户会话。
- 两套 `logout` 都删除各自完整文件。

### 自动登录测试

- 有效会话不调用登录。
- 失效会话加完整保存凭据只调用登录一次。
- 登录成功后读取请求只重放一次。
- 保存密码错误时只提交一次，随后清理密码并返回 `authentication_required`。
- 没有保存密码时不尝试登录。
- 两套 CLI 都能用仅传 `--password` 的方式复用保存账号，且缺少保存账号时明确失败。
- 网络、限流和验证码错误保留密码并返回原错误。
- 励销副作用操作和多步骤工作流不会被整体重放。

### dry-run 与输出测试

- dry-run 构造客户端时不调用凭据文件的 `load()`。
- dry-run 不调用网络、登录、验证码或凭据清理。
- `auth status`、成功 JSON、错误 JSON 和文档测试均不出现密码内容。
- 保留现有账号密码分开提问测试，并增加“已保存账号时只问密码”的文档契约。

实现完成后运行认证相关定向测试，再运行仓库完整测试套件。测试产生的临时凭据目录和
缓存必须清理，不改动当前工作区中与本任务无关的未跟踪文件。
