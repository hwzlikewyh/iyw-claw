---
name: iyw-image-workflows
description: 通过内置 Python CLI 独立检索 IYW 知识库、图片/报告/趋势/IP 资料，或调用已经确认的 IYW 图片工具接口，支持知识查询、分身生图、本地图片上传并自动违规检测、网络图片违规检测、变款、系列延伸、多图融合、编辑、扩图、高清修复、图案与线稿处理、格式转换、3D、视频和模特场景。用户提到 IYW 知识库、内部规范、品牌或 IP 手册、知识检索、生图、画图、修图、商品图、上传检测或 IYW 图片任务时使用；用户指定图片、上传图片或提供图片 URL 时默认优先变款，除非明确指定其他专用工具。
---

# IYW 图片工作流

只使用本 Skill 内置且已经验证的 CLI。先判断请求属于独立知识检索、图片/报告搜索、
分身生图、上传检测、固定图片工具还是任务查询，再执行对应命令。知识库和资料检索
是一项可单独使用的能力，不要求后续生图；生图也不强制先查询知识库。

## 图片生产入口

使用 `scripts/iyw_commerce.py` 的固定 `tool` 命令。不要调用已经失效的 `iywctl`，不要使用
`iywctl commerce`、`iywctl upload`、`iywctl task`、`list` 或 `describe`。

当前不要调用 `scripts/iyw_image.py` 的 `models`、`generate`、`edit`、
`upscale` 等命令；这些 Agent Image 路由尚未重新确认，不能作为生产接口。分身生图
固定使用 `scripts/iyw_commerce.py fission-generate`，不要用旧 `generate` 替代。

普通文生图优先使用本 Skill 的 `fission-generate`。如果用户指定图片、上传本地图片
或提供图片 URL，默认使用已封装的 `tool variation` 命令变款。这里的 `variation` 只是
本地 CLI 别名；CLI 会调用 `g_tools_generate_image`，并将 payload 的 `toolName` 固定为
`variation`，不要把它当作接口 operation。只有用户明确要求编辑、扩图、放大、线稿、
格式转换等专用动作时，才使用对应 `tool` 别名。用户明确要求 GPT Image 参数时，使用
`imagegen` Skill 的 `scripts/image_gen.py`。

优先使用 uv 在 Skill 目录内管理独立 Python 环境。在 PowerShell 中设置入口：

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\iyw-image-workflows"
$commerceCli = Join-Path $skillDir "scripts\iyw_commerce.py"
$knowledgeCli = Join-Path $skillDir "scripts\iyw_knowledge.py"
$searchCli = Join-Path $skillDir "scripts\iyw_search.py"
uv sync --project $skillDir --python 3.13
```

图片命令使用 `$commerceCli`，独立知识检索使用 `$knowledgeCli`，图片/报告搜索使用
`$searchCli`。统一通过
`uv run --project $skillDir --python 3.13 python <CLI>` 执行。`uv run` 会自动同步
`pyproject.toml` 并在 Skill 目录创建 `.venv`。只有 uv 不可用时，才使用当前环境中
已经确认可用的 Python 3.10 及以上版本运行对应 CLI。

## 连接与认证

- API origin 默认是 `https://gateway.iyw.cn`。
- 图片 API 在代码内固定追加 `/ai-application`；分身模型配置固定使用
  `/platform/basic/dict/getByKeys`；知识库检索固定使用
  `/ai-agent-new/api/knowledge/search`。这些入口都不接受 `--prefix`。
- agent 不得传入或猜测 `--prefix`，也不得使用 `/iyw-fusion-api/v1` 等路径。
- token 优先读取当前用户目录 `.iyw-claw/iyw-account-token.json` 中的
  `access_token`；没有非空账号 token 时，再按 `--token`、`IYW_TOKEN` 的顺序解析。
- agent 默认依赖账号文件，不要把 token 写进命令、payload、日志或回复。
- 除非用户明确指定测试环境，否则不要传 `--base-url` 或 `--token`。

所有 IYW API 请求只发送 `token` 请求头。不要发送 `Authorization`、
`tokenInfo`、`securityKey`，不要把任何认证值放进 JSON body。

## 独立查询知识库

知识库查询可单独完成用户请求，不需要创建图片任务。执行前读取
[references/tool-contracts.md](references/tool-contracts.md) 的知识库契约：

```powershell
uv run --project $skillDir --python 3.13 python $knowledgeCli `
  search --query "茶具设计规范" --limit 10 --dense-weight 0.5
```

`search` 固定向 `/ai-agent-new/api/knowledge/search` 提交 `category`、`query`、
`folderId`、`fileId`、`limit` 和 `denseWeight`。`--query` 必填；可按用户要求使用
`--category`、`--folder-id` 或 `--file-id` 限定范围。不要自行猜测文件或文件夹 ID。

只把 `ok: true` 视为成功。结果按服务端顺序返回 `count` 和 `results`，每项仅保留
正文、Markdown 正文、相关度、片段类型和文档名称等安全字段。回答独立知识查询时，
归纳相关片段并标注文档名称；不要输出 token usage、内部 metadata、临时签名 URL
或整份无关检索结果。

## 生图前按需检索

知识检索和生图是两个独立操作。不得在 `fission-generate`、`image_gen.py` 或其他
生图脚本内部强制串联查询。由 Agent 在构造最终提示词前自主判断：

- 请求依赖企业内部资料、品牌或 IP 手册、行业规范、材料工艺、结构安全、生产约束
  或合规要求时，优先先查知识库。
- 用户明确要求依据知识库、公司标准或既有设计规则时，必须先查询。
- 提示词和约束已经完整、纯创意创作、只按用户图片做明确编辑，或用户明确要求跳过
  查询时，直接生图。
- 查询失败、没有结果或结果不相关时，默认按用户原始要求继续生图；只有用户明确要求
  “必须依据知识库”时才停止并说明原因。

只提取与当前任务直接相关的事实和约束补充提示词，不要把完整检索结果原样塞入提示词，
也不要用知识库内容覆盖用户的明确要求。

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

## 图片输入优先级

按以下顺序选择图片生产入口：

1. 用户明确指定图片、上传本地图片或提供公开图片 URL，且没有指定专用动作：先上传/检测图片，再执行已封装的 `tool variation`；该别名内部调用 `g_tools_generate_image`，并将 payload 的 `toolName` 固定为 `variation`。
2. 用户明确要求编辑、扩图、高清修复、图层拆分、画质增强、提取图案、格式转换、线稿、配色、3D、视频或模特场景：执行对应专用 `tool`，不自动改成变款。
3. 没有图片输入且是纯文生图：执行 `fission-generate`。

`tool` 只接受固定别名，完整列表和 payload 见
[references/commerce-operations.md](references/commerce-operations.md)。搜索清单接口使用
`scripts/iyw_search.py`，固定别名和 JSON 示例见该 CLI 的帮助输出。搜索 CLI 固定
host/path，只发送 `token` 请求头；不得传入 Cookie、`securitykey`、`Authorization` 或任意 prefix。

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

## 执行固定图片工具

构造 payload 前必须读取
[references/commerce-operations.md](references/commerce-operations.md)。该文件记录
已有权威契约和清单中的固定图片工具：

- 变款：`g_tools_generate_image`，`toolName` 为 `variation`。
- 系列延伸：`g_tools_generate_image`，`toolName` 为 `extend`。
- 多图融合：`g_tools_generate_image`，`toolName` 为 `mix`，图片数量为 2 至 10。
- Commerce 放大：`upscaleImage`，`scale` 为 1 至 8 的整数。
- 其他清单工具通过 `tool <alias>` 调用，operation 和 `toolName` 由 CLI 固定填充。

把 JSON object 写入临时 UTF-8 文件，然后执行一次：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  tool variation `
  --input-file "C:\path\payload.json" `
  --no-progress
```

`tool variation` 是变款的标准入口；它会固定调用 `g_tools_generate_image` 并设置
`toolName`。不要用通用 `invoke` 绕过该封装。

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
  tool variation `
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

知识库检索 dry-run 可单独执行：

```powershell
uv run --project $skillDir --python 3.13 python $knowledgeCli `
  search --query "茶具设计规范" --dry-run
```

其 URL 必须精确为
`https://gateway.iyw.cn/ai-agent-new/api/knowledge/search`，且请求体不得包含 token。

## 结果与失败处理

- 只把 `ok: true` 视为 CLI 成功。
- `queued` 和 `running` 都不是最终成功。
- 只在状态为 `succeeded` 且存在图片 URL 时声明任务完成。
- 生成完成后，先在当前可用工具中解析展示工具：匹配后缀为 `show_image` 的名称
  （裸名，或命名空间形式 `mcp__<server>__show_image`；server 注册名为
  `iyw-claw-mcp`，即 `mcp__iyw-claw-mcp__show_image`）。
- 解析到：按服务端顺序对每个最终 HTTPS URL 调用一次，让结果以原生图片块显示在
  爱原物对话框中；该工具自己读取 URL，不要为了展示手动下载。
- 工具列表中没有：跳过展示，返回每个最终 HTTPS URL 并说明无法内联渲染。不要猜测
  名称变体，也不要声称已经展示。名称找不到的报错表示工具不存在，而非拼写错误，
  改名永远无效——第一次失败就走兜底。
- 只有用户明确要求保存到本地，或后续操作必须使用本地文件时，才下载结果图片。
- 创建请求超时或结果不确定时，只查询原 task ID，不要重建任务。
- 仅重试 `retryable: true` 的只读请求；不要自动重试收费创建请求。
- 知识库查询失败默认不阻塞后续生图；用户明确要求必须依据知识库时除外。
- 对用户只返回简洁状态、task ID，最终图片按上述展示规则处理。
- 不得暴露模型名、模型 ID、channel、provider、platform、`commerceType`、
  `toolType`、内部统计、token 或签名信息。
