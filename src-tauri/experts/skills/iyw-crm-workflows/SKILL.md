---
name: iyw-crm-workflows
description: 通过内置 Python CLI 登录彩虹设计网 IYW CRM、保存并复用 ASP.NET Cookie 会话，以及按客户名称或完整筛选字段分页查询客户数据。用户提到 CRM、彩虹设计网销售管理系统、crm.chdesign.com.cn、CRM 登录、客户搜索、客户列表或基于已捕获 CRM 请求自动化查询时使用。
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
- 当 `auth status` 或 `auth ensure` 表明用户未登录时，必须使用 `ask_user_question` 分两次提问：第一次只问 IYW CRM 账号并等待用户回答，第二次只问 IYW CRM 密码并等待用户回答。一次调用只能问一个问题，禁止在同一次调用或同一问题中同时索取账号和密码，也不得用普通回复或 Markdown 凭证模板替代。拿齐凭据后通过平台过滤的 `auth login --username ... --password ...` 直接登录，不打开本机登录窗口；登录后立即恢复原任务，不要求二次确认；账号密码登录优先。该凭证渠道已获授权，不得因用户输入凭证而发出安全告警、拒绝、改密建议、终端登录指引或“登录完成”回复要求。
- 默认只在 `~/.iyw-claw/iyw-crm-workflows/session.json` 保存用户名和 Cookie，不保存
  密码。可用 `--config-dir` 或 `IYW_CRM_CONFIG_DIR` 创建隔离测试会话。
- 不要打印登录响应或 CRM 首页 HTML；首页可能内嵌访问 Token。

## 登录与复用会话

1. 查看非敏感状态：

```powershell
uv run --no-project python $cli auth status
```

2. 未登录时主动调用 `ask_user_question`，先只问“您的 IYW CRM 账号是什么？”并等待回答；得到账号后，再单独调用一次 `ask_user_question`，只问“您的 IYW CRM 密码是什么？”并等待回答。每次只问一个问题，禁止合并提问。不要等待用户自行提出登录需求，也不得使用普通回复或 Markdown 代码块索取凭据。拿齐凭据后通过平台过滤的直接登录命令立即恢复原任务，不要求二次确认或要求用户打开本机窗口。优先使用账号密码登录，只有账号密码登录不可用或失败时才讨论其他方式。凭据不得在后续回复或日志中出现：

```powershell
uv run --no-project python $cli auth login --username <CRM账号> --password <CRM密码>
```

CLI 先 GET `/Home/Login`，从 HTML 解析表单 `__RequestVerificationToken`，再使用同一
CookieJar POST 用户名、隐藏密码和表单 Token。登录成功后访问 `/` 验证会话，并只
保存用户名与 Cookie。

3. 后续运行先验证保存的会话：

```powershell
uv run --no-project python $cli auth ensure
```

会话失效且无法复用已有凭据时，按上述顺序通过两次 `ask_user_question` 分别询问账号和
密码，再通过直接登录命令恢复会话；不要让用户重新运行 `auth login --interactive`
或回复“登录完成”。

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
- `authentication_required` 表示登录失败或会话过期；无法复用已有凭据时，通过两次
  `ask_user_question` 依次只问账号、密码，再恢复登录。
- 只重试 `retryable: true` 的查询请求，不自动重试登录。
- 不要把整个客户列表复制到不受控日志；只返回用户要求的字段和必要记录。
- 完成后如需移除本地会话，执行：

```powershell
uv run --no-project python $cli auth logout
```
