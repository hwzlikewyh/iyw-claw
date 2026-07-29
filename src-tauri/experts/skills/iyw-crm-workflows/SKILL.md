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
- CRM 的 HTTPS 虚拟主机不提供登录页，所以真实登录和客户查询使用 HTTP 明文链路。
- 只有用户明确允许直连 HTTP 后，才给真实命令传 `--allow-insecure-http`。没有该
  参数时 CLI 必须在发送任何请求前停止。
- 除非用户提供了已验证的新 origin，否则不要传 `--base-url`。

## 凭据与隐私

- 不要在对话中索取、回显或复用用户粘贴过的密码、Cookie、ASP.NET Token、JWT、
  refresh token 或 SAAS token。
- 要求用户在自己的终端运行 `auth login --interactive`；密码只能通过隐藏输入读取。
- 默认只在 `~/.iyw-claw/iyw-crm-workflows/session.json` 保存用户名和 Cookie，不保存
  密码。可用 `--config-dir` 或 `IYW_CRM_CONFIG_DIR` 创建隔离测试会话。
- 不要打印登录响应或 CRM 首页 HTML；首页可能内嵌访问 Token。
- 已经粘贴到对话或日志中的密码、Cookie 和 Token 应视为泄露并立即轮换。

## 登录与复用会话

1. 查看非敏感状态：

```powershell
uv run --no-project python $cli auth status
```

2. 让用户在本地终端交互登录：

```powershell
uv run --no-project python $cli --allow-insecure-http auth login --interactive
```

CLI 先 GET `/Home/Login`，从 HTML 解析表单 `__RequestVerificationToken`，再使用同一
CookieJar POST 用户名、隐藏密码和表单 Token。登录成功后访问 `/` 验证会话，并只
保存用户名与 Cookie。

3. 后续运行先验证保存的会话：

```powershell
uv run --no-project python $cli --allow-insecure-http auth ensure
```

会话失效时不要在 Agent 命令中传密码，也不要自动尝试旧密码；让用户重新运行
`auth login --interactive`。

## 查询客户

选择操作或构造高级筛选前读取
[references/commands.md](references/commands.md)。按名称分页查询：

```powershell
uv run --no-project python $cli --allow-insecure-http `
  api customer-search --text "正达" --page 1 --rows 15
```

用重复的 `--field KEY=VALUE` 覆盖已捕获筛选字段：

```powershell
uv run --no-project python $cli --allow-insecure-http `
  api customer-search --text "正达" `
  --field IsImportant=true --field IsDownApp=1
```

新筛选先 dry-run。dry-run 不读会话、不访问网络，也不需要 HTTP 授权参数：

```powershell
uv run --no-project python $cli --dry-run `
  api customer-search --text "正达"
```

## 结果处理

- 只把 `ok: true` 视为 CLI 成功。
- 客户查询成功数据必须含整数 `total` 和数组 `rows`。
- `authentication_required` 表示登录失败或会话过期；让用户在本地重新登录。
- 只重试 `retryable: true` 的查询请求，不自动重试登录。
- 不要把整个客户列表复制到不受控日志；只返回用户要求的字段和必要记录。
- 完成后如需移除本地会话，执行：

```powershell
uv run --no-project python $cli auth logout
```
