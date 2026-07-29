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
| `auth login --interactive` | 提示输入新用户名和隐藏密码，建立并保存会话。 |
| `auth login` | 复用保存的用户名，仅提示输入隐藏密码。 |
| `auth ensure` | 访问 CRM 首页验证保存的 Cookie。 |
| `auth logout` | 只删除本 Skill 的 `session.json`。 |

真实网络命令必须在子命令前传 `--allow-insecure-http`。`--dry-run` 不访问网络，不
需要该参数。

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
uv run --no-project python $cli --allow-insecure-http auth login --interactive
uv run --no-project python $cli --allow-insecure-http auth ensure
```

查询并仅保留需要的字段：

```powershell
$result = uv run --no-project python $cli --allow-insecure-http `
  api customer-search --text "正达" | ConvertFrom-Json
$result.data.rows | Select-Object Id, CustomerName, CaiHongID, StatusDesc
```

不要在日志中输出 Cookie、登录页、首页 HTML 或完整客户数据。响应中的
`ContactPhone` 当前可能是掩码；不要尝试通过未捕获接口绕过权限。
