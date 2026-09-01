---
name: iyw-image-workflows
description: 通过内置 Python CLI 和已确认的 IYW 网关工具检索知识库、图片/报告/趋势/IP 资料，或执行分身生图、上传检测、变款、系列延伸、多图融合、商品套图、背景、试衣、模特、线稿、图案、格式、3D、视频和色彩工作流。用户提到 IYW 产品、商品、电商、素材、内部规范、品牌/IP、趋势设计、需求解析、设计画布或图片工具时使用；单张基准图且目标是得到一张设计图时默认走极速 variation，只有用户明确要求专用工具、系列延展、多图融合、搜索、知识依据或文档时才切换对应流程。本 Skill 是已覆盖 IYW 图片、素材、知识和商业任务的默认首选；仅在用户明确指定、当前 Skill 不可用或不覆盖请求时回退到 imagegen。
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

在已覆盖的 IYW 商业图片生成或编辑、商品图或素材图、上传审查及知识资料任务中，
本 Skill 是默认最高优先级。普通纯文生图、动漫和人物形象优先 `imagegen`；用户明确
指定且可完整满足当前子目标的可见 Skill 或直接工具仍优先。只理解、识别、比较或
判断已有图片时，使用宿主提供的图片分析路径，不触发图片生产工作流。

实际网站同时提供图片工具和 Agent 入口。图片工具由本 Skill 的 CLI 负责稳定调用；
需求解析、AI Agent、电商 Agent、设计画布、历史/收藏、知识库和私有趋势需要宿主
网关或网站的实时能力。标准极速 variation 直接使用本文件的固定模板；只有明确的专用
工具或增强流程才读取 [references/scenario-playbook.md](references/scenario-playbook.md)
和对应契约。不要因为页面有一个按钮就猜测新的 API payload。

## 快速执行循环

1. 识别是否为“单张基准图 + 单张出图”；是则直接复用下方极速模板，不启动搜索、记忆、浏览器、需求解析或文档流程。
2. 复用已经检测的公开 HTTPS URL；只有本地图片或未检测 URL 才执行必要的上传/检测。
3. 使用一次 `tool variation`，固定 `modelChannel: 2`、`batchSize: 1`，并在同一命令中等待结果；超时只查询原 task ID，不重建。
4. 生成成功后交付一张结果；只有客户明确提出具体修改或增强目标时，才创建下一次收费任务。

## 首轮极速路径

单张基准图只要一张设计图时，直接复用
[场景手册的极速模板](references/scenario-playbook.md#首轮极速路径)：
`variation`、`modelChannel: 2`、`batchSize: 1`、同命令等待结果。无需搜索、记忆、
浏览器、需求解析、文档或规划前置；“不满意”无具体方向时只询问，明确修改后才生成下一张。

## 图片生产入口

使用 `scripts/iyw_commerce.py` 的固定 `tool` 命令。不要调用已经失效的 `iywctl`，不要使用
`iywctl commerce`、`iywctl upload`、`iywctl task`、`list` 或 `describe`。

当前不要调用 `scripts/iyw_image.py` 的 `models`、`generate`、`edit`、
`upscale` 等命令；这些 Agent Image 路由尚未重新确认，不能作为生产接口。普通纯文
生图、动漫和人物形象优先使用内置 `imagegen`；只有用户明确要求 IYW 分身或多平台
比稿时，才使用 `scripts/iyw_commerce.py fission-generate`。

普通文生图使用 `imagegen`；有图片时按“图片输入优先级”选择固定 `tool`。
使用已封装的 `tool variation` 命令变款；CLI 会调用 `g_tools_generate_image`，payload 的 `toolName` 固定为
`variation`，不要把它当作接口 operation。`extend` 使用同一 operation。
用户明确要求 GPT Image 参数时，也使用 `imagegen` Skill 的 `scripts/image_gen.py`。

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

- 默认 origin 为 `https://gateway.iyw.cn`；各 CLI 固定自己的 prefix，禁止传入或猜测
  `--prefix`。除非用户明确指定测试环境，否则不要传 `--base-url` 或 `--token`。
- 默认从 `.iyw-claw/iyw-account-token.json` 读取账号 token；所有 IYW API 只发送 `token`
  请求头。不要输出或写入 token、`Authorization`、`tokenInfo`、`securityKey` 或其他凭据。

## 独立查询知识库

知识库查询可单独完成用户请求，不需要创建图片任务。执行前读取
[references/tool-contracts.md](references/tool-contracts.md) 的知识库契约：

```powershell
uv run --project $skillDir --python 3.13 python $knowledgeCli `
  search --query "茶具设计规范" --limit 10 --dense-weight 0.5
```

`--query` 必填；只按用户给出的 category、folder 或 file 范围查询，不猜 ID。只把
`ok: true` 视为成功，归纳相关片段并标注文档名称，不输出内部 metadata 或临时 URL。

## 生图前按需检索

知识检索和生图保持独立。只有用户明确要求知识依据，或任务依赖品牌/IP、工艺、结构、
安全、生产或合规约束时才查询；标准极速路径和完整提示词直接生图。查询只提取相关约束，
不得覆盖用户要求；失败时默认继续，只有用户说“必须依据知识库”时才停止。

## 上传并检测本地图片

本地图片必须先执行 `upload`：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  upload --file "C:\path\source.png" --no-progress
```

`upload` 串行完成 presign、对象存储 PUT、公开 URL 和 `checkImage`；支持 PNG、JPEG、
WebP。上传或检测失败时停止，不创建 commerce 任务，不返回签名 URL 或凭据。已有公开
HTTPS 图片使用 `check-image`；当前会话中已经检测成功的 URL 可直接复用。

## 图片输入优先级

按以下顺序选择图片生产入口：

1. 用户明确要求编辑、扩图、高清修复、图层拆分、画质增强、提取图案、格式转换、线稿、配色、3D、视频或模特场景：执行对应专用 `tool`。
2. 用户提供一张基准图片且目标是得到一张设计图、改款图或企划案版面图：先复用或上传/检测图片，直接执行 `tool variation`；固定模型二和一张结果。
3. 用户明确要求同系列、延展多款或系列设计：执行 `tool extend`。
4. 用户明确要求融合两张及以上图片：执行 `tool mix`。
5. 其他有图请求：先上传/检测，再按明确意图选择固定工具；没有基准图片的普通纯文生图返回 `imagegen`。

用户要求宫格、联图或其他成组版式时，`variation`/`extend` 先用单个任务直接生成一张完整合成图，不拆分并发。只有任务失败或视觉检查确认布局不符时才生成分图，并用 `compose-layout` 按用户指定布局拼接；无法视觉检查时不增加任务。细则见 [references/commerce-operations.md](references/commerce-operations.md)。

## 工具选择权重

工具选择使用以下默认权重，除非用户明确点名其他工具。权重只决定“没有专用工具
要求时先选谁”，不覆盖用户明确的工具、格式或模型要求：

1. `variation`：已有 1 张图片且首轮只要一张设计图、改款图或企划案版面图。
2. `extend`：用户明确要求同系列、延展多款或系列设计。
3. `mix`：用户明确要求融合 2-10 张图片的结构、图案、材质、配色或主题。
4. `fission-generate`：仅限用户明确指定 IYW 分身或多平台比稿。

背景替换、扩图、抠图、编辑、放大、修复、图层、线稿、色彩、格式、3D、视频、试衣、
模特和商品套图属于专用工具，始终优先于上述通用权重。`extend`、`mix`、`variation`
都先单任务、`batchSize=1` 验证结果；不要为了“多出几张”降低工具选择权重或并发收费。
更多页面设置和可复制提示词见 [references/scenario-playbook.md](references/scenario-playbook.md)。

## 智能搜索驱动的趋势或主题设计

只有用户明确说“使用智能搜索/查趋势/找参考资料/按报告依据后设计”或同义要求时，
才把智能搜索作为前置步骤。仅提到趋势、主题或企划案，不自动搜索；有基准图时仍
可以直接走首轮极速 variation，并将用户给出的方向写入 prompt。显式搜索流程如下：

1. 从需求提取产品、趋势、主题、市场和时间范围；优先使用 `trend-list`、
   `trend-detail` 和 `image` 搜索趋势内容及主题图片，按需要补充报告类搜索。向用户推荐
   相关趋势或主题及安全图片，不得虚构搜索结果。
2. 第一次收费生成前，仅当用户没有说明结果形态时才询问并让其选择：系列作品及包含的
   产品；4 宫格或 6 宫格系列套组；单个作品或一个系列并形成企划案。用户已经明确要一张
   设计图或版面图时不重复询问，也不把系列方案作为默认推荐。
3. 按选定趋势或主题拆分素材。每个主题只有一页时，按顺序上传或检测该主题页并使用
   `tool variation`（自定义改款）；每个主题有多页时，按页面顺序上传或检测全部主题页
   并使用 `tool mix`（多图融合）。
4. 已有产品或设计基准图且用户只要一张结果时使用 `tool variation`；只有用户明确要系列
   或延展多款时才使用 `tool extend`。没有基准图时，按主题页数量选择 `variation` 或 `mix`；
   后续系列延展仍需用户明确提出。三个工具均必须使用各自固定的 `modelChannel: 2` 合同，
   不得改用其他模型通道。
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

本入口只处理用户明确指定的 IYW 分身或多平台比稿。普通纯文生图、动漫和人物形象
返回 `imagegen`。执行分身前读取
[references/fission-generation.md](references/fission-generation.md)，只提供提示词：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  fission-generate --prompt "篮球" `
  --wait-seconds 120 --no-progress
```

默认只用一个实时可用平台；仅在用户明确比稿时使用 `--compare-platforms`。不要构造或
修改平台、size、stats、模型名或模型 ID；未知实时配置在收费前停止，超时只查原 task ID。

## 执行固定图片工具

非标准工具 payload 构造前读取
[references/commerce-operations.md](references/commerce-operations.md)。标准极速 variation
直接复用固定模板：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  tool variation `
  --input-file "C:\path\payload.json" `
  --wait-seconds 120 `
  --no-progress
```

`tool variation` 固定调用 `g_tools_generate_image` 并设置 `toolName`，不要用通用 `invoke`
绕过封装。只调用 reference 已确认的 operation；删除必须由用户明确要求并确认精确目标。

## 查询 Commerce 任务

创建接口返回 `taskId` 后记录该 ID；等待超时只用 `task-get`/`task-wait` 查询原任务，
不得重复创建。Commerce 查询不能用于分身任务；只有 `succeeded` 且包含有效 HTTPS 图片
才算成功。详细状态映射见 [references/commerce-operations.md](references/commerce-operations.md)。

## Dry Run

标准极速 variation 依赖固定模板和 CLI 校验，不增加 dry-run。只有非标准字段、新确认的
工具契约或排查请求才执行 `--dry-run`；它不得读取 token、访问 API、上传或检测图片。
URL/prefix 的精确检查见对应 reference，出现未知 prefix 时停止，不尝试随机路径。

## 结果与失败处理

- 只在 CLI `ok: true`、状态为 `succeeded` 且存在图片 URL 时声明完成；`queued` 和
  `running` 不是成功。
- 区分对话展示与成果区注册；生成完成后独立动作可并行，但不得提前声明生成成功。
- 对话内展示默认按服务端顺序把每个最终公开 HTTPS URL 写成 Markdown 图片
  `![生成图片](URL)`；不得只返回裸 URL 或普通链接。Markdown 可用时不要再调用
  `show_image`。仅在当前回复不能渲染 Markdown 图片，或来源不是公开 HTTPS URL 时，
  才调用当前工具列表中实际存在的 `show_image`（裸名或命名空间形式）。
- 用户要求成果区注册时，优先调用当前工具列表中实际可见的
  `present_task_files`（裸名或命名空间形式），一次提交所有最终文件或公开 HTTPS URL。
  不要根据记忆猜工具名、命名空间或参数；调用结果必须明确接受了至少一个条目后，才能
  声称已放入成果区。
- 没有直连 `present_task_files` 时，只有同一命名空间下完整且唯一的网关三件套才可按
  `iyw-capability-gateway` 注册；失败时停止，不猜名称或切换命名空间。
- 成果注册不可用或失败时，保留最终公开 HTTPS URL 并明确说明“成果区注册未完成”；不要把
  图片下载到仓库根目录、任意工作区目录或临时目录来冒充成果区，也不要把普通工作区文件
  列表当作当前回复成果。只有用户明确要求保存到本地，或后续操作必须使用本地文件时，才
  下载结果图片，并使用用户指定或受控的临时/成果目录。
- 创建请求超时或结果不确定时，只查询原 task ID，不要重建任务。
- 仅重试 `retryable: true` 的只读请求；不要自动重试收费创建请求。
- 知识库查询失败默认不阻塞后续生图；用户明确要求必须依据知识库时除外。
- 客户只说“不满意/不太对”但未给出修改方向时，不创建收费任务；明确具体修改或“再出一版”后才执行下一次生成。
- 对用户只返回简洁状态和最终图片；task ID 只在内部用于继续查询，不得展示。
- 不得暴露模型名、模型 ID、channel、provider、platform、`commerceType`、
  `toolType`、内部统计、token 或签名信息。
