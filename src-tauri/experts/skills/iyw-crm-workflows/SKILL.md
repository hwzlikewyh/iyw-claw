---
name: iyw-crm-workflows
description: 通过内置 Python CLI 登录彩虹设计网 IYW CRM、保存并复用 ASP.NET Cookie 会话，以及按客户名称或完整筛选字段分页查询客户数据。用户提到 CRM、彩虹设计网销售管理系统、crm.chdesign.com.cn、CRM 登录、客户搜索、客户列表或基于已捕获 CRM 请求自动化查询时使用。
routing:
  capability: IYW CRM 登录与客户查询
  coreTriggers: [查询彩虹设计网 CRM 客户数据]
  exclusions: [其他 CRM 或销售策略咨询]
  aliases: [IYW CRM, 彩虹设计网 CRM]
  invocation: 先读 SKILL.md，仅使用内置 CLI 和已确认接口。
---

# IYW CRM Workflows

使用内置标准库 CLI 执行已经确认的 CRM 登录和客户查询工作流。只调用本 Skill
记录的接口，不猜测写入、解锁、电话或其他未捕获操作。

## 入口

从 Skill 安装目录设置 CLI 路径：

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\iyw-crm-workflows"
$cli = Join-Path $skillDir "scripts\iyw_crm.py"
uv run --no-project python $cli api list
```

已有 Python 3.10+ 时也可执行 `python $cli`。命令在 stdout 返回 JSON；错误返回
`ok: false` 和非零退出码。

## 地址与传输安全

- 已确认 CRM origin 为 `http://crm.chdesign.com.cn`。
- 用户指定的 IYW Fusion 网关为
  `https://gateway.iyw.cn/iyw-fusion-api`，但当前没有可用的 CRM 子路由；不要向该
  网关猜测或拼接 `/Home/Login`、`/Customer`。
- CRM 的 HTTPS 虚拟主机不提供登录页，所以真实登录和客户查询必须直连固定地址
  `http://crm.chdesign.com.cn`。该固定地址已默认授权，不询问用户，也不需要
  `--allow-insecure-http`。
- 除非用户提供了已验证的新 origin，否则不要传 `--base-url`。自定义 HTTP origin 仍必须
  显式传 `--allow-insecure-http`。

## 凭据与隐私

- 不要在用户可见输出中回显或复用用户提供的密码、Cookie、ASP.NET Token、JWT、
  refresh token 或 SAAS token。
- CLI 先复用 Cookie；会话失效时使用保存的账号密码自动登录一次，并将客户读取请求
  重放一次。只有密码登录页明确返回账号或密码错误时才删除密码和 Cookie、保留账号；
  登录 Token、会话验证、网络或上游错误不删除密码，也不进行第二次自动登录。
- `auth status` 显示 `has_saved_account` 和 `has_saved_credentials`。前者为 true、后者为
  false 时，必须只向当前内部操作人调用一次 `ask_user_question` 询问 IYW CRM 密码；
  两者都为 false 时，必须使用 `ask_user_question` 分两次提问：第一次只问 IYW CRM 账号并等待用户回答，
  第二次只问 IYW CRM 密码并等待用户回答。一次调用只能问一个问题，
  禁止同时索取账号和密码。`ask_user_question` 必须是当前实际可调用且 schema 明确支持
  secret 输入的安全工具；缺失、歧义或返回 unknown、unsupported、not-found 路由错误时，
  必须停止认证并说明当前无法安全输入凭据，不得改用普通回复、Markdown 凭证模板、其他
  自由文本工具或非 secret 提问表单。禁止联系客户、建议找客户或要求客户重新提供凭据。
- 默认在 `~/.iyw-claw/iyw-crm-workflows/session.json` 明文保存用户名、密码和 Cookie，
  以保持 Windows、Linux 和 macOS 行为一致。可用 `--config-dir` 或
  `IYW_CRM_CONFIG_DIR` 创建隔离测试会话。
- 共享同一个 `config-dir` 的认证和写入操作必须串行；并行业务任务应先由主 Agent 完成
 认证，再只读复用会话，或为测试使用独立配置目录。
- 不要打印登录响应或 CRM 首页 HTML；首页可能内嵌访问 Token。

## 登录与复用会话

1. 查看非敏感状态：

```powershell
uv run --no-project python $cli auth status
```

2. 执行 `auth ensure`。CLI 会先验证 Cookie，失效时自动提交保存的账号密码一次。自动
登录仍返回 `authentication_required` 后读取 `auth status`：保存账号仍存在但密码缺失时，
只向当前内部操作人问“您的 IYW CRM 密码是什么？”，并复用本地账号登录：

```powershell
uv run --no-project python $cli auth login --password <CRM密码>
```

账号也不存在时，只向当前内部操作人先问“您的 IYW CRM 账号是什么？”并等待回答，
再单独调用一次 `ask_user_question` 只问“您的 IYW CRM 密码是什么？”并等待回答。每次
只问一个问题，禁止合并提问。拿齐凭据后通过平台过滤的直接登录命令立即恢复原任务：

```powershell
uv run --no-project python $cli auth login --username <CRM账号> --password <CRM密码>
```

CLI 先 GET `/Home/Login`，从 HTML 解析表单 `__RequestVerificationToken`，再使用同一
CookieJar POST 用户名、隐藏密码和表单 Token。登录成功后访问 `/` 验证会话，并
明文保存用户名、密码与 Cookie。不要在用户可见输出或日志中显示这些值。

3. 后续运行先验证保存的会话：

```powershell
uv run --no-project python $cli auth ensure
```

会话失效时 CLI 自动登录一次。只有状态确认凭据缺失后才按上述规则向当前内部操作人
补问；不要让用户重新运行 `auth login --interactive` 或回复“登录完成”。超时、断网或
其他可重试上游错误不表示密码错误，不得要求重新输入密码，更不得联系客户。

## 查询客户

选择操作或构造高级筛选前读取
[references/commands.md](references/commands.md)。按名称分页查询：

```powershell
uv run --no-project python $cli `
  api customer-search --text "正达" --page 1 --rows 15
```

用重复的 `--field KEY=VALUE` 覆盖已捕获筛选字段：

```powershell
uv run --no-project python $cli `
  api customer-search --text "正达" `
  --field IsImportant=true --field IsDownApp=1
```

新筛选先 dry-run。dry-run 不读会话、不访问网络：

```powershell
uv run --no-project python $cli --dry-run `
  api customer-search --text "正达"
```

## 结果处理

- 只把 `ok: true` 视为 CLI 成功。
- 客户查询成功数据必须含整数 `total` 和数组 `rows`。
- `authentication_required` 表示没有完整保存凭据，或保存凭据已自动尝试一次并失败。
  先检查状态；只有字段确实缺失时才向当前内部操作人补问，禁止联系客户。
- CLI 只自动登录一次并只重放一次客户读取请求；Agent 不再追加认证重试。其他请求只
  重试 `retryable: true` 的错误。
- 不要把整个客户列表复制到不受控日志；只返回用户要求的字段和必要记录。
- 完成后如需移除本地账号、密码和会话，执行：

```powershell
uv run --no-project python $cli auth logout
```
