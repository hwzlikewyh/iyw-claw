# IYW Website API Requests

业务端点从 [爱原物接口索引](iyw-api-index.md) 按需查找。除图片生成/处理和通用上传，
其余业务接口统一使用本工具。不要新增对应 MCP，也不走 capability search/read/invoke。

Call `fetch_iyw_url` directly through its advertised identity. It does not need
capability search/read. Use an endpoint and parameter contract supplied by the
user, official documentation, or an observed request. Existing dedicated tools
remain the preferred route when they cover the task.

## Request

- `description`: 1-120 字符，说明当前具体动作，如“查询产品列表”。由界面显示，不进入 HTTP 请求。
- `timeout_seconds`: 默认 60，范围 1-900；文案/方案 90，知识库批量/递归操作和打包按来源可设 600。它不是业务参数，主机不转发到 body/query。
- `url`: absolute HTTPS URL on `iyw.cn` or any subdomain level, with any API path.
  URL credentials and fragments are rejected. All HTTPS ports are supported.
- `method`: `GET`, `POST` (default), `PUT`, `PATCH`, `DELETE`, `HEAD`, or
  `OPTIONS`. Case-insensitive; surrounding whitespace is ignored.
- `query`: optional object of string, number, boolean, or null values, or arrays
  of these. Arrays create repeated keys; nulls are omitted. Values are URL
  encoded and appended to any query parameters already present in `url`.
- `body_type`: `json` (default), `form`, or `text`.
- `body`: JSON accepts any JSON value, including explicit null. `form` requires
  an object with the same scalar/array rules as query and sends URL-encoded form
  data. `text` sends a raw string, including XML. GET and HEAD must omit body.
- `headers`: optional string-valued headers, applied after body encoding. May
  override defaults such as `content-type`, `accept`, `origin`, or `referer`.
  Host, credentials/cookies, framing, connection, and proxy headers are reserved.
  Invalid names/values and duplicate case-insensitive names are rejected.

The host reads the current iyw-claw login token for each call. Do not ask the
user for a token or provide tokens/cookies as tool arguments or header overrides.
An absent login returns `authentication_error` before sending the API request.

对于 `https://www.iyw.cn/gateway/`、`/msgapi/` 和 `/api/`，主机同时注入 `iyuanwu_token`
Cookie，并默认使用门户 Origin/Referer。仍禁止代理提供 Cookie 或访问主机凭证。

Default headers follow the supplied website request: JSON content type,
`application/json, text/plain, */*` accept, `zh-CN,zh;q=0.9` language,
Origin `https://tu.iyw.cn`, Referer `https://tu.iyw.cn/trendPop?from=portal`,
and the supplied Chrome 152 Windows browser/client-hint headers. Body encoding
sets the appropriate content type; an explicit `headers.content-type` wins.
For XML use `body_type: "text"` and `content-type: application/xml`.
Multipart/file upload bodies are not supported by this tool.
上传任意文件用 [upload_iyw_file](iyw-upload.md)，不能把二进制塞入 fetch 的 body。

The user-provided trend query is represented as:

```json
{
  "description": "查询趋势主题列表",
  "url": "https://gateway.iyw.cn/theme-activity/api/Trend/GetTrendList",
  "method": "POST",
  "body": {
    "keywords": "",
    "orderBy": 0,
    "market": -1,
    "pageIndex": 4,
    "pageSize": 99,
    "categoryType": 0
  }
}
```

Only perform operations authorized by the user. A generic HTTP tool may change
data or incur charges; its name does not imply a read-only operation.

## Response And Failures

The MCP `outputSchema` fixes this envelope for HTTP results and tool execution
errors. Existing `status`, `content_type`, and `body` fields are preserved:

```json
{
  "ok": true,
  "status": 200,
  "content_type": "application/json",
  "body_type": "json",
  "body": {"code": 1, "data": []},
  "error": null
}
```

- `ok`: HTTP 2xx with a complete response. It does not interpret business codes;
  inspect `body.code` and `body.message` according to the called API.
- `status`: HTTP status, or null when no response was received.
- `content_type`: server content type, or an empty string when unavailable.
- `body_type`: `json`, `text`, `base64`, or `empty`. JSON is parsed, recognized
  UTF-8 text is returned as a string, binary/non-UTF-8 data is Base64. HEAD and
  empty responses return `empty` with null body; JSON null remains type `json`.
- `body`: the API's original business payload, without stripping its envelope.
  Unavailable/incomplete bodies are null. Response headers and request
  credentials are not included. Text and parsed JSON redact the current token.
- `error`: null on HTTP success, otherwise an object with `code`, `message`,
  `execution_status`, and `retryable` (false; there are no automatic retries).
  Codes include `invalid_request`, `authentication_error`, `client_error`,
  `http_error`, `timeout`, `transport_error`, `response_too_large`, and `cancelled`.

All failures set MCP `isError=true` and `ok=false`. HTTP non-2xx preserves its
response body. A failure during body reading preserves received HTTP status and
content type. Input/client/authentication failures report `not_started`, complete
HTTP errors report `responded`, and uncertain transport/cancellation/body-read
failures report `unknown`. The envelope is in `structuredContent` and the MCP
text content. MCP transport authentication, authorization, and routing errors
occur before the tool executes and retain their protocol error format.

- HTTPS redirects are followed only within `iyw.cn` and its subdomains, up to
  five hops. A disallowed target or exhausted redirect limit returns the 3xx
  response without contacting that target.
- The HTTP timeout defaults to 60 seconds; timeout_seconds accepts 1-900. Request and response bodies are limited to
  2 MiB before response Base64 encoding. HEAD ignores the advertised resource
  content length since it has no response body.
- 流式聊天会等到响应结束才返回文本，不能显示逐 token 流。超出超时或大小限制时返回失败信封，
  已发起的会话/任务可能仍在运行，应查原始 ID，不能自动重新提交。

补充篇的 L0-L3、开发 tokenInfo、签名及方法冲突见 [调用约定补充](iyw-api-access-contracts.md)。
主机保留 tokenInfo 认证头，代理不能提供；不继承网站的自动重试，也不猜签名盐值。
- Timeout, cancellation, transport errors, or oversized/incomplete responses can
  occur after execution. Do not blindly resubmit writes or paid actions.
