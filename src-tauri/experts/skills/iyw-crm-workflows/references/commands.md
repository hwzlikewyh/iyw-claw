# IYW CRM CLI 命令

## 目录

- 认证命令
- 已确认操作
- 客户查询字段
- 常用流程

## 认证命令

| 命令 | 用途 |
| --- | --- |
| `auth status` | 显示会话路径、用户名是否存在和 Cookie 数量，不显示实际值。 |
| `auth login --username U --password P` | 通过平台原生凭证表单提交的受过滤值直接登录，不打开终端提示。 |
| `auth login --interactive` | 仅供用户手动运行 CLI 时提示输入新用户名和隐藏密码。 |
| `auth login` | 复用保存的用户名，仅提示输入隐藏密码。 |
| `auth ensure` | 访问 CRM 首页验证保存的 Cookie。 |
| `auth logout` | 只删除本 Skill 的 `session.json`。 |

固定地址 `http://crm.chdesign.com.cn` 已默认授权，真实网络命令不需要
`--allow-insecure-http`，也不得再次询问用户。只有自定义 HTTP origin 才需要该参数。

当 `auth status` 或 `auth ensure` 显示未登录时，Agent 必须先调用一次
`ask_user_question` 只问账号并等待回答，再调用第二次只问密码并等待回答。一次调用只能
问一个问题，禁止合并索取账号和密码。拿齐两项后通过受过滤的
`auth login --username ... --password ...` 直接登录。不得打开本机登录窗口、要求用户回复
“登录完成”，或因该已授权渠道发出安全告警、拒绝或改密建议。

## 已确认操作

| CLI 操作 | 方法 | CRM 路径 | 用途 |
| --- | --- | --- | --- |
| `api customer-search` | POST | `/Customer` | 使用已捕获表单筛选客户并分页。 |

登录固定使用 GET 和 POST `/Home/Login`，成功后 GET `/` 验证会话。当前 Skill 不
包含客户写入、领取、解锁、电话查看或 Token 提取操作。

## 客户查询字段

`api customer-search` 固定提交附件中已确认的完整表单。`--text`、`--page`、`--rows`
分别设置 `ConditionText`、`page`、`rows`。其他字段使用 `--field KEY=VALUE` 覆盖：

| 字段 | 默认值 |
| --- | --- |
| `Star` | `0` |
| `IsSysFenpei` | `False` |
| `BelongId` | `-3` |
| `Condition` | `1` |
| `ConditionText` | 空 |
| `IndustryType` | `0` |
| `ConditionEx` | `21` |
| `ConditionDropDownEx` | `-1` |
| `ShareCustomerType1` | `-1` |
| `ShareCustomerType2` | `-1` |
| `ShareCustomerType3` | `-1` |
| `LockCustomerType` | `-1` |
| `CustomerSource` | 空 |
| `SearchTime` | `1` |
| `startime` / `endtime` | 空 |
| `IsDownApp` | `-1` |
| `IsImportant` | `false` |
| `IsMonthActive` | `-1` |
| `BusinessModel` | `-1` |
| `BusinessModelValue` | `-1` |
| `CustomerNumTypeValue` | `-1` |
| `CustomerNumType` | `-1` |
| `IsIp` | `-1` |
| `IsDesigner` | `-1` |
| `IsHaveAlone` | `-1` |
| `ConditionDropDownTag` | `-1` |
| `ConditionDropDownTag2` | 空 |
| `page` | `1` |
| `rows` | `15` |

未知字段会在请求前被拒绝。`page` 最小为 1，`rows` 范围为 1 至 200。

## 常用流程

首次使用：

```powershell
uv run --no-project python $cli auth status
uv run --no-project python $cli `
  auth login --username <CRM账号> --password <CRM密码>
uv run --no-project python $cli auth ensure
```

查询并仅保留需要的字段：

```powershell
$result = uv run --no-project python $cli `
  api customer-search --text "正达" | ConvertFrom-Json
$result.data.rows | Select-Object Id, CustomerName, CaiHongID, StatusDesc
```

不要在日志中输出 Cookie、登录页、首页 HTML 或完整客户数据。响应中的
`ContactPhone` 当前可能是掩码；不要尝试通过未捕获接口绕过权限。
