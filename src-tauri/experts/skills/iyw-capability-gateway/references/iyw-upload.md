# 通用文件上传

直接调用 `upload_iyw_file`，通常只填 `path` 和 `description`，不需要 search/read/invoke。文件类型不限，单文件最大 1 GiB，即 1,073,741,824 字节，恰好达到上限可上传，超过即在网络请求前拒绝。

优先使用真实绝对路径，不要求文件位于工作区内。中文目录、中文文件名、空格和括号均支持：

```json
{"path":"D:/Downloads/设计方案（最终版）.pdf","description":"上传设计方案"}
```

也支持相对会话工作目录的路径：

```json
{"path":"交付文件/设计方案（最终版）.pdf","description":"上传设计方案"}
```

## 入参

| 字段 | 必填 | 含义 |
| --- | --- | --- |
| `path` | 是 | MCP 主机上可读的普通文件，不限工作区；优先填绝对路径，相对路径基于会话工作目录 |
| `description` | 是 | 1-120 字符当前动作，例“上传设计方案文档” |
| `name` | 否 | 通常不填。覆盖返回结果中的文件名，默认本地 basename；支持中文，不能有 `/`、`\`、`:` 或控制字符。不会改变读取哪个文件 |
| `mime_type` | 否 | 通常不填。不确定时省略，默认 `application/octet-stream`；填入时使用 `application/pdf` 等标准 MIME，不是扩展名或中文文件类型 |

```json
{"description":"上传设计方案文档","path":"deliverables/design.pdf","mime_type":"application/pdf"}
```

```json
{"description":"上传项目压缩包","path":"deliverables/source.zip","mime_type":"application/zip"}
```

支持 PDF、Office、压缩包、图片、音频、视频及其他二进制文件。文件大小包括全部原始字节，不是 Base64 编码后大小。目录先由当前任务工具打包为文件。允许工作区外的绝对路径、`../` 路径及指向工作区外的软链接；最终目标必须是宿主可读的普通文件。文件不存在、不可读、不是普通文件或超限时不会请求上传凭证。

普通文件采用流式上传，不会把整个文件一次读入内存；上传期间不要修改源文件。存储连接超时 30 秒，单次上传请求最多等待 30 分钟；若 Agent 或代理设置了更短的工具调用超时，仍可能提前取消。通用上传上限不代表下游业务接口上限，例如知识库和图片工具仍需遵守各自限制。

桌面用户 Downloads 或其他目录中的文件可直接用绝对路径上传，无需复制进工作区；成果区的本地文件路径也可直接使用。服务器模式路径属于服务器，不是客户端电脑。HTTP URL、Data URL、Base64 不作为本工具输入；先取得 MCP 主机可读的本地文件。

### 中文路径如何填写

- 保留原始中文，不做 URL 编码，不把中文改成 `%E4%…`。
- 不在字符串值内再包 shell 引号；JSON 自身的双引号照常保留。
- Windows 推荐写 `D:/workspace/交付文件/设计方案.pdf`；使用反斜杠时，JSON 中写成 `D:\\workspace\\交付文件\\设计方案.pdf`。
- 相对路径基于本次会话工作区，不随另一次 shell 命令的 `cd` 改变。
- 不用改成英文名来绕过错误。存储对象名由主机生成随机 ID，中文文件名不会写进签名请求头；先根据错误确认失败阶段。

## 结果与后续

成功结果含 `ok:true`、`url`、`name`、`mime_type`、`size_bytes`。`url` 是无临时签名的 HTTPS 地址。只有存储服务接受上传后才返回成功；不返回 PUT 签名、STS 或 token。错误为 `ok:false`、`error`，同时设置 MCP 错误标记。

- 上传 URL 可作为 `fetch_iyw_url` 产品、客户趋势、文档等接口中已确认的文件字段；上传本身不会保存业务记录或触发解析。
- 交付文件时用 `present_task_files`；上传不自动注册最终产物。
- 图片生成/编辑的本地输入由 `generate_iyw_image` 内部上传，无需额外先调用本工具。图片工具的输入上限仍为 20 MiB。
- 文件上传后链接持有者可访问，仅上传用户指定的文件；不要为“上传项目”扫描或附带整个工作区。
- 上传或请求取消可能留下对象。错误中 `execution_status=unknown` 时不能声称远端无文件，也不能盲目重试。存储拒绝时说明失败，禁止改端点重复上传。

## 失败后怎么处理

先读取 `error.code`、`error.message` 和 `error.execution_status`，不要只凭一次失败宣布整个上传通道不可用。

| 错误 | 下一步 |
| --- | --- |
| `invalid_request` / `not_started` | 根据提示修正参数。确认 `path`、`description` 都是字符串；文件确实存在于 MCP 主机且可读，路径未编码、未额外加引号。相对路径找不到时核实真实绝对路径，无需将文件搬进工作区；核实原因后更正一次，不轮流猜路径 |
| `file_too_large` / `not_started` | 文件超过 1 GiB；按任务需要压缩或拆分，不能只改文件名 |
| `presign_failed` / `not_started` | 文件校验已通过，但取得上传许可失败，尚未上传文件字节。查看 HTTP/业务码，检查该主机登录、网络、代理和服务状态；改 `name` 或绝对/相对路径无效 |
| `invalid_response` / `not_started` | 预签名服务返回了工具不能使用的地址；保留错误信息并排查服务协议，不猜备用端点 |
| `storage_rejected` / `responded` | 存储明确拒绝；保留 HTTP 状态码并排查权限、签名或服务状态，不靠改名重复上传 |
| `transport_error` 或 `cancelled` / `unknown` | 结果不确定，远端可能已有文件。停止重试并报告，不自动切通道 |

若目标只是交付文件，可保留完整文件，并通过 `present_task_files` 提交本地最终产物；这不等于云端上传成功。若下游接口必须使用公网 URL，应明确说明当前步骤受阻，待登录、网络或服务恢复后再继续，不能伪造 URL 或改用未经确认的上传服务。

预签名沿用已确认的 `POST /ai-application/api/microModel/PreSignedUrl` + `objectKey`，再向返回地址 PUT 文件二进制。PUT 是存储协议内部动作，不是额外业务 MCP，也不经 fetch；认证凭证由主机持有。
