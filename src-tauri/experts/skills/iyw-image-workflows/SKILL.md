---
name: iyw-image-workflows
description: 通过内置 Python CLI 调用已经确认的 IYW 图片接口，支持分身生图、本地图片上传并自动违规检测、网络图片违规检测、电商变款、系列延伸、多图融合、图片放大、commerce 任务查询，以及在已有权威 JSON 契约时调用指定 commerce operation。用户提到生图、分身生图、画图、修图、商品图、上传图片、检测图片、改款、变款、系列延伸、多图融合、放大、upscale 或 IYW 图片任务时使用；不得猜测接口路径、prefix、operation 或 payload。
---

# IYW 图片工作流

只使用本 Skill 内置且已经验证的 CLI。先判断请求属于分身生图、上传检测、已知
commerce 工作流还是任务查询，再执行对应命令。

## 唯一生产入口

使用 `scripts/iyw_commerce.py`。不要调用已经失效的 `iywctl`，不要使用
`iywctl commerce`、`iywctl upload`、`iywctl task`、`list` 或 `describe`。

当前不要调用 `scripts/iyw_image.py` 的 `models`、`generate`、`edit`、
`upscale` 等命令；这些 Agent Image 路由尚未重新确认，不能作为生产接口。分身生图
固定使用 `scripts/iyw_commerce.py fission-generate`，不要用旧 `generate` 替代。

普通文生图优先使用本 Skill 的 `fission-generate`。图片编辑，或用户明确要求
GPT Image 参数时，使用 `imagegen` Skill 的 `scripts/image_gen.py`；该脚本通过
`/iyw-fusion-api/v1` 调用接口并复用爱原物账号 token。

优先使用 uv 在 Skill 目录内管理独立 Python 环境。在 PowerShell 中设置入口：

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\iyw-image-workflows"
$commerceCli = Join-Path $skillDir "scripts\iyw_commerce.py"
uv sync --project $skillDir --python 3.13
```

后续命令统一使用
`uv run --project $skillDir --python 3.13 python $commerceCli`。`uv run` 会自动同步
`pyproject.toml` 并在 Skill 目录创建 `.venv`。只有 uv 不可用时，才使用当前环境中
已经确认可用的 Python 3.10 及以上版本运行 `$commerceCli`。

## 连接与认证

- API origin 默认是 `https://gateway.iyw.cn`。
- 图片 API 在代码内固定追加 `/ai-application`；分身模型配置固定使用
  `/platform/basic/dict/getByKeys`。两者都不接受 `--prefix`。
- agent 不得传入或猜测 `--prefix`，也不得使用 `/iyw-fusion-api/v1` 等路径。
- token 优先读取当前用户目录 `.iyw-claw/iyw-account-token.json` 中的
  `access_token`；没有非空账号 token 时，再按 `--token`、`IYW_TOKEN` 的顺序解析。
- agent 默认依赖账号文件，不要把 token 写进命令、payload、日志或回复。
- 除非用户明确指定测试环境，否则不要传 `--base-url` 或 `--token`。

所有 IYW API 请求只发送 `token` 请求头。不要发送 `Authorization`、
`tokenInfo`、`securityKey`，不要把任何认证值放进 JSON body。

## 上传并检测本地图片

本地图片必须先执行 `upload`：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  upload --file "C:\path\source.png" --no-progress
```

该命令固定执行以下完整流程：

1. 请求 `api/microModel/PreSignedUrl` 获取签名 URL。
2. 使用签名 URL 向对象存储执行二进制 `PUT`，对象存储请求不携带 token。
3. 去掉签名查询参数，得到公开图片 URL。
4. 请求 `api/microModel/checkImage` 检测图片。
5. 仅在上传和检测都成功后返回 `image_url` 与 `checked: true`。

上传或检测失败时立即停止，不要继续创建 commerce 任务。不要向用户返回签名
URL、签名参数、对象存储凭据或 token。

支持 `.png`、`.jpg`、`.jpeg`、`.webp`。默认自动生成
`AI/img/日期/随机文件名.扩展名` 格式的 object key。

## 检测已有网络图片

对已有公开 HTTPS 图片单独执行：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  check-image --image-url "https://example.com/image.png" --no-progress
```

只有检测接口返回成功时才能把该 URL 放进后续 commerce payload。

## 分身生图

执行前读取
[references/fission-generation.md](references/fission-generation.md)。只提供提示词，
不要让 agent 构造或修改 `platform`、`size`、`stats`、模型名或模型 ID：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-generate --prompt "篮球" `
  --wait-seconds 120 --no-progress
```

CLI 固定执行以下流程：

1. 从 `/platform/basic/dict/getByKeys` 读取实时 `model_options`。
2. 按配置顺序选择标签为“分身”的模型，并套用已经确认的默认参数。
3. 向 `api/microModel/v2/batch` 只提交一次收费创建请求。
4. 保存返回的 `groupId` 和全部 task ID。
5. 使用 `api/microModel/GetDetails` 分别轮询每个 task ID。
6. 按 batch 任务顺序返回并直接展示全部 HTTPS 图片。

实时配置出现 CLI 尚未支持的新分身时，在创建任务前停止，不要猜参数。创建请求
超时或响应不确定时不要重新生成；只查询已经获得的 task ID。

只创建任务而不等待时设置 `--wait-seconds 0`。后续查询使用：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-task-get --task-id "602862275132395520" --no-progress
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-task-wait `
  --task-id "602862275132395520" `
  --task-id "602862274985594880" `
  --wait-seconds 120 --no-progress
```

`fission-models` 仅用于读取当前分身数量和标签。不要向用户暴露返回的模型内部配置、
创建响应中的余额、micro、platform 或 task 详情中的模型信息。

## 执行 Commerce 操作

构造 payload 前必须读取
[references/commerce-operations.md](references/commerce-operations.md)。该文件只记录
已有权威契约的四类操作：

- 变款：`g_tools_generate_image`，`toolName` 为 `variation`。
- 系列延伸：`g_tools_generate_image`，`toolName` 为 `extend`。
- 多图融合：`g_tools_generate_image`，`toolName` 为 `mix`，图片数量为 2 至 10。
- Commerce 放大：`upscaleImage`，`scale` 为 1 至 8 的整数。

把 JSON object 写入临时 UTF-8 文件，然后执行一次：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  invoke g_tools_generate_image `
  --input-file "C:\path\payload.json" `
  --no-progress
```

CLI 将 operation 固定拼接为：

```text
IYW_API_BASE_URL + /ai-application/api/commerce/ + operation
```

operation 只允许字母、数字和下划线，禁止路径、URL 和 `..`。未在 reference 中
记录 payload 的 operation，只有用户或权威接口文档提供完整 JSON 契约时才允许
调用；不得根据 operation 名称猜字段。

`removeTaskOrImage` 只有在用户明确要求删除并确认精确目标后才能调用，同时必须传
`--confirm-destructive`。不得自动删除或清理任务。

## 查询 Commerce 任务

创建接口返回 `taskId` 后，记录该 ID。不要因为等待超时而重复创建收费任务。

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  task-get --task-id "602450311860195328" --no-progress
uv run --project $skillDir --python 3.13 python $commerceCli `
  task-wait --task-id "602450311860195328" `
  --wait-seconds 120 --no-progress
```

Commerce 任务查询固定使用 `api/commerce/getCommerceTaskDetail`，不得用于查询分身
任务。状态映射如下：

- `process: 10`：`succeeded`
- `process: 20` 或 `30`：`failed`
- 其他非终态：`queued` 或 `running`

成功结果只使用 `images[].image`、`images[].cover` 或 `images[].url` 中的 HTTPS
图片地址，并保持服务端顺序。

## Dry Run

对新 payload 先执行 `--dry-run`，检查 URL 与 JSON body。dry-run 不读取 token、
不访问 API、不上传文件、不执行图片检测。

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  invoke g_tools_generate_image `
  --input-file "C:\path\payload.json" `
  --dry-run --no-progress
```

确认输出 URL 必须以
`https://gateway.iyw.cn/ai-application/api/` 开头。若出现其他 prefix，立即停止，
不要尝试随机路径。

分身生图 dry-run：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-generate --prompt "篮球" --dry-run --no-progress
```

其 URL 必须精确为
`https://gateway.iyw.cn/ai-application/api/microModel/v2/batch`。

## 结果与失败处理

- 只把 `ok: true` 视为 CLI 成功。
- `queued` 和 `running` 都不是最终成功。
- 只在状态为 `succeeded` 且存在图片 URL 时声明任务完成。
- 生成完成后，按服务端顺序对每个最终 HTTPS URL 调用 `show_image`，让结果以原生
  图片块显示在爱原物对话框中；`show_image` 会读取 URL，不要为了展示手动下载。
- 只有用户明确要求保存到本地，或后续操作必须使用本地文件时，才下载结果图片。
- 创建请求超时或结果不确定时，只查询原 task ID，不要重建任务。
- 仅重试 `retryable: true` 的只读请求；不要自动重试收费创建请求。
- 对用户只返回简洁状态、task ID，并通过 `show_image` 直接展示最终图片。
- 不得暴露模型名、模型 ID、channel、provider、platform、`commerceType`、
  `toolType`、内部统计、token 或签名信息。
