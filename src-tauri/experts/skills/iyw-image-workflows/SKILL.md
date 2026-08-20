---
name: iyw-image-workflows
description: 通过内置 Python CLI 独立检索 IYW 知识库、图片/报告/趋势/IP 资料，或调用已经确认的 IYW 图片工具接口，支持知识查询、分身生图、本地图片上传并自动违规检测、网络图片违规检测、变款、系列延伸、多图融合、编辑、扩图、高清修复、图案与线稿处理、格式转换、3D、视频和模特场景。用户提到 IYW 知识库、内部规范、品牌或 IP 手册、知识检索、生图、画图、修图、商品图、上传检测、使用智能搜索获取趋势或主题素材并设计，或 IYW 图片任务时使用；图片请求按图片输入优先级路由，其中有基准图片且指定趋势或主题时优先系列延伸，其他普通有图请求默认变款，明确指定专用工具时使用对应工具。本 Skill 是已覆盖 IYW 图片、素材与知识任务的默认首选；仅在用户明确指定、当前 Skill 不可用或不覆盖请求时回退到 imagegen。
routing:
  capability: IYW 图片素材知识首选工作流
  coreTriggers: [IYW 知识资料或图片生成编辑上传审查]
  exclusions: [只需理解已有图片, 无需 IYW 数据接口的普通文本, 用户明确指定其他可见能力]
  aliases: [IYW 图片, IYW 知识库]
  invocation: 除显式指定外优先于 imagegen；先读 SKILL.md 并按图片输入路由。
---

# IYW 图片工作流

只使用本 Skill 内置且已经验证的 CLI。先判断请求属于独立知识检索、图片/报告搜索、
分身生图、上传检测、固定图片工具还是任务查询，再执行对应命令。知识库和资料检索
是一项可单独使用的能力，不要求后续生图；生图也不强制先查询知识库。

在已覆盖的 IYW 图片生成或编辑、商品图或素材图、上传审查及知识资料任务中，
本 Skill 是默认最高优先级。用户明确指定且可完整满足当前子目标的可见 Skill 或
直接工具仍优先。`imagegen` 仅用于用户明确选择、需要 GPT Image 专用参数、
本 Skill 不可用或不覆盖请求的情况。只理解、识别、比较或判断已有图片时，使用
宿主提供的图片分析路径，不触发图片生产工作流。

## 图片生产入口

使用 `scripts/iyw_commerce.py` 的固定 `tool` 命令。不要调用已经失效的 `iywctl`，不要使用
`iywctl commerce`、`iywctl upload`、`iywctl task`、`list` 或 `describe`。

当前不要调用 `scripts/iyw_image.py` 的 `models`、`generate`、`edit`、
`upscale` 等命令；这些 Agent Image 路由尚未重新确认，不能作为生产接口。分身生图
固定使用 `scripts/iyw_commerce.py fission-generate`，不要用旧 `generate` 替代。

普通文生图使用 `fission-generate`；有图片时按“图片输入优先级”选择固定 `tool`。
使用已封装的 `tool variation` 命令变款；CLI 会调用 `g_tools_generate_image`，payload 的 `toolName` 固定为
`variation`，不要把它当作接口 operation。`extend` 使用同一 operation。
用户明确要求 GPT Image 参数时，使用 `imagegen` Skill 的 `scripts/image_gen.py`。

`fission-generate` 默认只向一个平台下发并优先使用通道四；通道四不可用时，回退到实时配置顺序中的第一个可用平台。只有明确要求多平台比稿时才传 `--compare-platforms`。

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

1. 用户明确要求编辑、扩图、高清修复、图层拆分、画质增强、提取图案、格式转换、线稿、配色、3D、视频或模特场景：执行对应专用 `tool`。
2. 用户提供一张基准图片并指定趋势或主题：先上传/检测，优先执行 `tool extend`；明确失败后只回退一次 `tool variation`。
3. 其他有图请求：先上传/检测，再执行已封装的 `tool variation` 命令变款；该别名内部调用 `g_tools_generate_image`。没有基准图片的纯文生图仍执行 `fission-generate`。

用户要求宫格、联图或其他成组版式时，`variation`/`extend` 先用单个任务直接生成一张完整合成图，不拆分并发。只有任务失败或视觉检查确认布局不符时才生成分图，并用 `compose-layout` 按用户指定布局拼接；无法视觉检查时不增加任务。细则见 [references/commerce-operations.md](references/commerce-operations.md)。

## 智能搜索驱动的趋势或主题设计

用户提到“使用智能搜索拿到图片及内容，再根据趋势或主题进行设计”或同义要求时，
把智能搜索作为必需前置步骤，并按以下顺序执行：

1. 从需求提取产品、趋势、主题、市场和时间范围；优先使用 `trend-list`、
   `trend-detail` 和 `image` 搜索趋势内容及主题图片，按需要补充报告类搜索。向用户推荐
   相关趋势或主题及安全图片，不得虚构搜索结果。
2. 第一次收费生成前，若用户尚未说明，先询问并让其选择：系列作品及包含的产品；
   4 宫格或 6 宫格系列套组；单个作品或一个系列并形成企划案。优先推荐系列作品或
   系列企划案，但不得替用户静默决定。
3. 按选定趋势或主题拆分素材。每个主题只有一页时，按顺序上传或检测该主题页并使用
   `tool variation`（自定义改款）；每个主题有多页时，按页面顺序上传或检测全部主题页
   并使用 `tool mix`（多图融合）。
4. 已有产品或设计基准图时，仍优先使用 `tool extend` 形成系列；没有基准图时先用主题页
   通过 `variation` 或 `mix` 得到种子作品，选择系列结果后再使用 `extend` 延展。三个工具
   均必须使用各自固定的 `modelChannel: 2` 合同，不得改用其他模型通道。
5. 4 宫格或 6 宫格必须让每格各有一个符合主题的作品，并保持产品语言、配色和材质体系
   一致，形成系列套组；企划案必须同时组织主题依据、产品组合、配色、材质、工艺和作品图。

用户明确要求智能搜索但搜索失败或没有可用趋势、主题图片及内容时，不要跳过搜索后直接
收费生成；先说明结果，并询问是放宽搜索条件还是改用用户提供的主题页。详细路由见
[references/commerce-operations.md](references/commerce-operations.md)。

`tool` 只接受固定别名，完整列表和 payload 见
[references/commerce-operations.md](references/commerce-operations.md)。搜索清单接口使用
`scripts/iyw_search.py`；先执行 `example <alias>` 获取安全 JSON 模板，按需求修改后
再执行 `search <alias> --input-file <path> --dry-run`。17 个固定别名、字段和结果
格式见 [references/tool-contracts.md](references/tool-contracts.md)。搜索 CLI 固定
host/path，只发送 `token` 请求头；不得传入 Cookie、`securitykey`、
`Authorization` 或任意 prefix。

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
2. 校验标签为“分身”的平台并套用已经确认的默认参数；默认只选通道四，缺失时回退到
   实时配置顺序中的第一个可用平台。
3. 向 `api/microModel/v2/batch` 只提交一次收费创建请求。
4. 保存返回的 `groupId` 和全部 task ID。
5. 使用 `api/microModel/GetDetails` 分别轮询每个 task ID。
6. 按 batch 任务顺序返回并直接展示全部 HTTPS 图片。

比稿时选择全部可用分身平台并将通道四排在第一位；通道四不可用时保持实时配置顺序。
命令示例见 [references/fission-generation.md](references/fission-generation.md)。

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
- 生成完成后，先在当前可用工具中解析展示能力。若存在后缀为 `show_image` 的直连
  工具（裸名或命名空间形式），按服务端顺序对每个最终 HTTPS URL 调用一次；该工具
  自己读取 URL，不要为了展示手动下载。
- 若没有直连 `show_image`，但当前工具列表在同一命名空间下同时暴露
  `search_iyw_capabilities`、`read_iyw_capability`、
  `invoke_iyw_capability`，则遵循 IYW Capability Gateway 流程：搜索图片展示能力、
  读取 schema，再按返回的精确 capability ID 对每个 URL 调用一次。不得猜 ID，也不得
  拼接不同命名空间的三个 gateway。
- 两条路径都不存在时，跳过展示，返回每个最终 HTTPS URL 并说明无法内联渲染。
  名称找不到表示直连工具不存在；检查一次 gateway 后立即兜底，不要猜名称变体，
  也不要声称已经展示。
- 只有用户明确要求保存到本地，或后续操作必须使用本地文件时，才下载结果图片。
- 创建请求超时或结果不确定时，只查询原 task ID，不要重建任务。
- 仅重试 `retryable: true` 的只读请求；不要自动重试收费创建请求。
- 知识库查询失败默认不阻塞后续生图；用户明确要求必须依据知识库时除外。
- 对用户只返回简洁状态、task ID，最终图片按上述展示规则处理。
- 不得暴露模型名、模型 ID、channel、provider、platform、`commerceType`、
  `toolType`、内部统计、token 或签名信息。
