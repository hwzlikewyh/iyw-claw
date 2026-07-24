# IYW 已确认 HTTP 契约

图片 API 固定使用 `IYW_API_BASE_URL + /ai-application/ + 接口路径`。默认 API
origin 为 `https://gateway.iyw.cn`。分身模型配置是唯一例外，固定使用 origin 下的
`/platform/basic/dict/getByKeys`。agent 不得提供或猜测 prefix。

## 认证

所有网关 API 只发送以下认证头：

```http
token: <access_token>
```

token 优先来自当前用户目录 `.iyw-claw/iyw-account-token.json` 的
`access_token`；没有非空账号 token 时，再按 `--token`、`IYW_TOKEN` 的顺序解析。
不得发送 `Authorization`、`tokenInfo` 或 `securityKey`。

## 已确认接口

| CLI 命令 | 方法 | 接口路径 | 请求内容 |
| --- | --- | --- | --- |
| `upload` 第一步 | POST | `/api/microModel/PreSignedUrl` | `objectKey` |
| `upload` 第二步 | PUT | 接口返回的签名 URL | 图片二进制，不发送 token |
| `upload` 第三步 | POST | `/api/microModel/checkImage` | `image` |
| `check-image` | POST | `/api/microModel/checkImage` | `image` |
| `invoke` | POST | `/api/commerce/{operation}` | operation 对应 JSON object |
| `task-get` / `task-wait` | POST | `/api/commerce/getCommerceTaskDetail` | `taskId` |
| `fission-models` | POST | origin `/platform/basic/dict/getByKeys` | `nameSpace`、`keys` |
| `fission-generate` | POST | `/api/microModel/v2/batch` | `prompt`、`jsonData`、`models` |
| `fission-task-get` / `fission-task-wait` | POST | `/api/microModel/GetDetails` | `taskId` |

`PreSignedUrl` 成功响应的 `data` 是带查询签名的 HTTPS PUT URL。上传完成后去掉
查询参数得到公开 URL，再调用 `checkImage`。任何一步失败都不得继续创建 commerce
任务。

分身生图契约详见
[fission-generation.md](fission-generation.md)。分身任务不得使用 commerce 的
`getCommerceTaskDetail` 查询。

## 输出信封

CLI 成功输出：

```json
{
  "ok": true,
  "data": {}
}
```

CLI 失败输出：

```json
{
  "ok": false,
  "error": {
    "code": "invalid_input",
    "message": "...",
    "retryable": false
  }
}
```

不得把 HTTP 2xx 直接等同于业务成功；上游 JSON `code` 必须为 `1`。
