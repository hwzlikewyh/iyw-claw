# IYW 已确认 HTTP 契约

图片 API 固定使用 `IYW_API_BASE_URL + /ai-application/ + 接口路径`。默认 API
origin 为 `https://gateway.iyw.cn`。分身模型配置固定使用 origin 下的
`/platform/basic/dict/getByKeys`，独立知识库检索固定使用 origin 下的
`/ai-agent-new/api/knowledge/search`。agent 不得提供或猜测 prefix。

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
| `iyw_knowledge.py search` | POST | origin `/ai-agent-new/api/knowledge/search` | `category`、`query`、`folderId`、`fileId`、`limit`、`denseWeight` |

`PreSignedUrl` 成功响应的 `data` 是带查询签名的 HTTPS PUT URL。上传完成后去掉
查询参数得到公开 URL，再调用 `checkImage`。任何一步失败都不得继续创建 commerce
任务。

分身生图契约详见
[fission-generation.md](fission-generation.md)。分身任务不得使用 commerce 的
`getCommerceTaskDetail` 查询。

## 知识库检索

知识库接口可独立使用，不要求前置或后续图片任务。默认请求体为：

```json
{
  "category": 0,
  "query": "茶具设计规范",
  "folderId": null,
  "fileId": null,
  "limit": 10,
  "denseWeight": 0.5
}
```

`query` 必须是非空字符串，`limit` 必须大于 `0`，`denseWeight` 必须在 `0` 到 `1`
之间。`folderId` 是可选整数，`fileId` 是可选字符串；只有用户或权威上下文提供精确
ID 时才使用。

知识库有两级业务响应：网关外层 `code` 必须为 `1`，`data.result.code` 必须为 `0`。
内层 `data.result.data.result_list` 必须为数组，`count` 必须为非负整数。CLI 保持服务端
顺序，并只返回：

- `count`
- 片段 `id`、`score`、`content`、`md_content`、`chunk_type`
- 文档 `doc_id`、`doc_name`、`doc_type`

不得返回 `request_id`、`token_usage`、`collection_name`、`doc_meta`、图片预览地址、
文档源 URL 或任何临时签名 URL。

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

不得把 HTTP 2xx 直接等同于业务成功；上游 JSON `code` 必须为 `1`。知识库检索还
必须验证内层 `result.code` 为 `0`。
