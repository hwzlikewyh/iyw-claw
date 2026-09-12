# 图片生成与处理工具

全部生成/处理调用 `generate_iyw_image`；不要把图片 API 作为 fetch 的替代执行路径。任务查询、历史、素材、收藏和其他业务仍走 `fetch_iyw_url`，见 [图片任务管理](iyw-api-image-admin.md)。只读本次 type 对应行和示例；核对来源或契约差异时再读 [图片接口证据](iyw-image-api-source.md)。

## 类型与参数

`images` 接受 HTTPS URL、工作区本地路径、Data URL，或 `{path|url|base64|data,mimeType,role,name}` 对象；原始 Base64 需 MIME。主机上传本地图片，单图上限 20 MiB，最多 10 张。`prompt` 描述目标，`parameters` 放所选操作的业务字段。新原生操作不同时接受顶层 images 和同名 parameters 图片字段。

| type | 后端操作 | 参数与区别 |
| --- | --- | --- |
| `fission` | `microModel/v2/batch` | 文生图；prompt 必填，按现有 `models/jsonData` 契约，非摘要 items[] |
| `variation` / `extend` | `commerce/g_tools_generate_image` | 单图 + prompt；主机设置 toolName/modelChannel/batchSize |
| `mix` | `commerce/g_tools_generate_image` | 2-10 图 + prompt；按输入顺序说明角色 |
| `pattern-apply` / `material-product` / `ip-apply` | `commerce/g_tools_generate_image` | 分别需 product/material 或 product/jsonData；从产品/IP 业务结果取得 |
| `free-imitation` | `commerce/fission` | 单图；`model:free`，`stats.width/height/strength` 非负 |
| `outpaint` | `commerce/outpainting` | 单图；`top/right/bottom/left` 各为 0-1 |
| `super-resolution` | `commerce/SuperResolution` | 单图；`upscale` 为 2 或 4 |
| `split-layers` / `separate-layers` | `commerce/f_tools` / `g_tools_generate_image` | 前者 `model:extract_layers`；后者主机设置 `toolName:seperate_layers` |
| `enhance` | `commerce/EnhanceImage` | 单图；`enhanceType` 为 1/2，`model` 为已知整数 |
| `extract-pattern` / `repeat-horizontal` | `commerce/g_tools_generate_image` | 单图；前者需 prompt，后者左右连续 |
| `convert` | `commerce/convert` | 单图；`inputFormat/outputFormat`，现有支持 png/jpg/jpeg/webp/gif/bmp |
| `line-extraction` | `commerce/lineExtraction` | 单图；model 为 realistic/canny，正整数 batch_size，stats.reference |
| `color-transfer` | `commerce/g_tools_generate_image` | 两图；productImg/styleImg，resolution 为 2K/4K |
| `image-to-3d` | `commerce/ImageTo3D` | 单图；stats.format 整数，stats.MultiViewImages 为带 ViewImageUrl 的对象数组 |
| `video` | `commerce/videoGenerator` | 单图 + prompt；ratio、duration 4-15 秒、mode normal/hd |
| `model-scene` / `background` | `commerce/modelScene` | 1-10 图 + prompt；size 比例、resolution standard/4K |
| `modify` | `commerce/imageModification` | 图片 + prompt；可传非负 strength |
| `seed-edit` | `commerce/SeedEdit` | 图片 + prompt，融合创款/指令编辑 |
| `blend` | `commerce/blend` | 2-10 图；prompt 可选，原生融合而非 mix 通道 |
| `erase` / `watermark-erase` | `commerce/erase` / `watermarkEraser` | 图片 + parameters.mask（HTTPS 蒙版）；erase 可带 prompt/payOrderNo；不猜订单号 |
| `extract` / `lineart` | `commerce/extraction` / `lineart` | 图片；lineart 可传 style，区别于旧 line-extraction |
| `vectorize` / `three-views` | `commerce/vectorizeImage` / `threeVisions` | 图片，矢量化/三视图 |
| `bleed-line` | `commerce/bleedLine` | 图片 + size；size 的单位/结构须来自实际页面，不能猜毫米/像素 |
| `upscale` / `super-upscale` | `commerce/upscaleImage` / `SuperUpscale` | 图片；可传非负 upscale 和已确认 op，不能提供 token |
| `video-auto-director` / `video-remake-director` | `commerce/videoAutoDirector` / `videoRemakeDirector` | 图片 + prompt；ratio/duration/mode 按实际页面配置 |
| `extract-color` / `save-color` | `commerce/extractColor` / `saveColor` | 前者图片；后者非空 colors 数组，结构来自提色结果 |
| `detect-grid` / `build-extract-prompts` | `commerce/detectImageGrid` / `buildExtractPrompts` | 图片，返回网格信息或提取提示词 |
| `classify-intent` | `commerce/classifyCanvasIntent` | 非空 keys 数组，不要求图片；键取自已知画布上下文 |
| `background-remove` | `microModel/GetImageSegment` | 单图；主机映射为 imageUrl |
| `check-image` | `microModel/checkImage` | 单图，沿用已确认 image 字段；仅用户要求检查时调用，不自动前置到其他图片操作 |
| `micro-upscale` / `micro-upscale-image` | `microModel/upscale` / `upscaleImage` | 单图映射 imageUrl；amount/device/payMethod/typeId 从实际页面确定 |
| `g-tools` / `f-tools` | `commerce/g_tools` / `f_tools` | 前者 prompt，可选 images/model；后者 content，price/type 来自已确认请求；不猜通道参数 |
| `micro-generate` / `micro-variation` | `microModel/v2/generate` / `variation` | 前者 prompt，可选图片及 modelChannel/ratio/batchSize/tool；后者图片 + prompt |
| `scheme-generate` | `ai-chat/api/designScheme/generateImage` | schemeId + prompt；schemeId 来自设计稿业务列表 |
| `faddish` | `ai-application/faddish/generate` | prompt，可选图片 |
| `generate` / `edit` | Fusion | 文生图优先 generate；显式选择 edit 时需有参考图；均从模型目录选择准确 model ID，无须先尝试平台接口 |

新增原生类型仅验证文档已明确的图片、提示词和基础类型；未提供枚举的字段不硬编码猜测。输入仍以官网当前契约为准；上游明确拒绝要保留原错误，未知状态先查任务。`/api/generate_mask` 的域名不明，尚不能通过工具直接生成蒙版。

## 结果与恢复

- `status/images/task_id/metadata/delivery` 为输出。原生操作完整业务结果在 `metadata.result`，色号、网格、提示词、矢量文件、3D 或视频链接不能只看 images。
- 原生异步任务的 `metadata.query` 给出已知查询路径；恢复查询用 fetch。返回 task_id 但没有已确认查询接口时保留运行中状态，不能宣称成功。
- 批量 `requests` 最多 8 项，`count` 每项 1-4，总执行次数最多 16；每次执行可能扣点。批量成功和失败分别报告；数据类操作结果保留在各项 `runs[].metadata.result`。
- 超时/取消/查询失败不重新扣点提交，先查原 task_id。原生查询失败时保留任务 ID 和 poll_error。
- 视频、3D、矢量文件链接如不在 images 中，通过 metadata.result 读取并按需用 present_task_files 交付；不要声称已经在图片预览中展示。

```json
{"type":"erase","prompt":"移除选区杂物并延续背景纹理","images":["assets/source.png"],"parameters":{"mask":"https://已上传蒙版的真实地址"}}
```

```json
{"type":"extract-color","images":["assets/palette.png"]}
```

## 既有操作与示例

Choose the route from the task:

| Task | Preferred type |
| --- | --- |
| Text-to-image, without source images | `generate` through Fusion `images/generations` |
| Redesign or modify one source image | `variation` |
| Fuse 2-10 reference images | `mix`, preserving input order |
| Four-panel grids or same-series extension from one base image | `extend` |

Fusion `edit` (`images/edits`) remains available when explicitly selected and
requires at least one source image. `generate` and `edit` do not require a prior
platform attempt or failure. `fission` remains an explicitly selectable platform
operation. Select the matching specialized operation for background, outpaint,
super-resolution and similar tasks.

Before `generate`, `auto` without images, or explicit `edit`, call
`list_iyw_image_models` with `{}`. Select from the returned descriptions,
capabilities, prices and task requirements: `generate` needs
`capabilities.image_generation=true`; `edit` needs
`capabilities.image_editing=true`. Pass the exact returned `id` in
`parameters.model`. Missing IDs and display names are rejected; the host does
not choose a default model. Reuse the catalog for the same task or batch;
an empty or failed lookup supplies no model. Platform operations need no Fusion
model lookup. Replace `MODEL_ID_FROM_CATALOG` in examples before calling.

`auto` uses `generate` without images; with one image it uses `extend` for
series, extension, four-panel or 2x2 wording, and `variation` otherwise; with
multiple images it uses `mix`. Prefer an explicit type when the intent is known.
An `extend` request needs one base image: describe the four-panel layout or series
in its prompt. Without a reference, create the requested composition with
`generate`. Keep multiple references for fusion; for independent series derived
from multiple bases, use separate `requests` items, each with one base image.
Do not discard references to force the single-image extension interface.

Inspect the returned operation when explaining routing. After a timeout,
transport error or uncertain submission, query the original task ID when available;
do not blindly resubmit or switch routes. Deliver ordinary successful images from
returned status, URLs and delivery metadata. Inspect visuals for requested review,
comparison, visual acceptance or integration into a composed deliverable.
For intermediate images used in a PPT/report/webpage, set
`delivery.registerArtifact: false` and register only the final deliverable.

**Timeouts:** omit `wait.timeoutSeconds` for 600 seconds on platform requests and
polling, or 300 seconds on `generate`/`edit`. The agent can explicitly override
either, including values above 600; prefer the defaults or longer for slow tasks.
Do not shorten waits merely to return sooner. Each batch item has its own wait.
`0` means submit without polling on the platform; Fusion keeps its default timeout.

```json
{"type":"generate","prompt":"白底陶瓷茶壶，现代东方风，产品摄影","parameters":{"model":"MODEL_ID_FROM_CATALOG"},"wait":{"timeoutSeconds":900}}
```

```json
{"type":"variation","prompt":"只把包身改成深绿色防水尼龙，保留版型、拉链、提手和视角","images":["https://example.com/bag.png"]}
```

```json
{"type":"extend","prompt":"保持原图结构和材质语言，设计四宫格同系列花瓶，每格一种配色","images":[{"url":"https://example.com/vase.png","role":"primary"}],"parameters":{"ratio":"4:3","batchSize":1}}
```

```json
{"type":"mix","prompt":"以第1张产品结构、第2张趋势配色融合成一件可生产餐盘","images":[{"url":"https://example.com/product.png","role":"structure"},{"url":"https://example.com/trend.png","role":"style"}]}
```

When Fusion editing is explicitly selected:

```json
{"type":"edit","prompt":"以原图为参考自由重绘为超现实拼贴海报，重新设计透视和构图，保留主体标识，右侧留出标题区域","images":[{"base64":"...","mimeType":"image/png","role":"source"}],"parameters":{"model":"MODEL_ID_FROM_CATALOG"}}
```

```json
{"type":"background","prompt":"浅木桌面和自然接触阴影，主体边缘完整","images":["https://example.com/product.png"],"parameters":{"size":"1:1","resolution":"standard"}}
```

```json
{"type":"super-resolution","images":["https://example.com/low-res.png"],"parameters":{"upscale":4}}
```

```json
{"type":"line-extraction","images":["https://example.com/product.png"],"parameters":{"model":"canny","batch_size":1,"stats":{"reference":"https://example.com/product.png"}}}
```

```json
{"type":"image-to-3d","images":["https://example.com/product.png"],"parameters":{"stats":{"format":1,"MultiViewImages":[]}}}
```

```json
{"type":"video","prompt":"镜头从正面缓慢环绕，展示材质高光","images":["https://example.com/product.png"],"parameters":{"ratio":"16:9","duration":8,"mode":"normal"}}
```

商品套图与 AI 试衣缺少完整提交契约；不得猜 endpoint。出血线、色号提取已根据新资料增加显式类型，见上表；其参数与旧页面本地处理设置不能混用。

For a local path use `"images":["assets/product.png"]`; for raw base64 use an
object with `base64` and `mimeType`; for a Data URL pass it as the string source.
For IYW image tools, non-URL sources become public HTTPS before execution;
`edit` uses their validated bytes directly. The decoded input limit is 20 MiB
and HTTP image URLs are rejected.
