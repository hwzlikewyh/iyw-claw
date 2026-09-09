# 通用文件上传

直接调用 `upload_iyw_file`，不需要 search/read/invoke。文件类型不限，单文件最大 50 MiB，即 52,428,800 字节，恰好达到上限可上传，超过即在网络请求前拒绝。

## 入参

| 字段 | 必填 | 含义 |
| --- | --- | --- |
| `path` | 是 | 当前 MCP 主机工作区内的普通文件，绝对路径或相对工作区路径 |
| `description` | 是 | 1-120 字符当前动作，例“上传设计方案文档” |
| `name` | 否 | 文件名及扩展名，默认本地 basename；不能有目录或控制字符 |
| `mime_type` | 否 | 合法 MIME，默认 `application/octet-stream`；不是文件类型白名单 |

```json
{"description":"上传设计方案文档","path":"deliverables/design.pdf","mime_type":"application/pdf"}
```

```json
{"description":"上传项目压缩包","path":"deliverables/source.zip","mime_type":"application/zip"}
```

支持 PDF、Office、压缩包、图片、音频、视频及其他二进制文件。文件大小包括全部原始字节，不是 Base64 编码后大小。目录先由当前任务工具打包为文件。路径越过工作区、软链接指向工作区之外、文件不可读、文件超限时不会请求上传凭证。

桌面用户 Downloads 中的文件若不在本次会话工作区，先按用户授权复制到当前工作区。服务器模式路径属于服务器，不是客户端电脑。HTTP URL、Data URL、Base64 不作为本工具输入；先取得真实本地文件。

## 结果与后续

成功结果含 `ok:true`、`url`、`name`、`mime_type`、`size_bytes`。`url` 是无临时签名的 HTTPS 地址。只有存储服务接受上传后才返回成功；不返回 PUT 签名、STS 或 token。错误为 `ok:false`、`error`，同时设置 MCP 错误标记。

- 上传 URL 可作为 `fetch_iyw_url` 产品、客户趋势、文档等接口中已确认的文件字段；上传本身不会保存业务记录或触发解析。
- 交付文件时用 `present_task_files`；上传不自动注册最终产物。
- 图片生成/编辑的本地输入由 `generate_iyw_image` 内部上传，无需额外先调用本工具。图片工具的输入上限仍为 20 MiB。
- 文件上传后链接持有者可访问，仅上传用户指定的文件；不要为“上传项目”扫描或附带整个工作区。
- 上传或请求取消可能留下对象。错误中 `execution_status=unknown` 时不能声称远端无文件，也不能盲目重试。存储拒绝时说明失败，禁止改端点重复上传。

预签名沿用已确认的 `POST /ai-application/api/microModel/PreSignedUrl` + `objectKey`，再向返回地址 PUT 文件二进制。PUT 是存储协议内部动作，不是额外业务 MCP，也不经 fetch；认证凭证由主机持有。
