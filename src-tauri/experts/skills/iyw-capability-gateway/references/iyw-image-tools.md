# 图片生成与处理工具

## 模型参数能力

`list_iyw_image_models` 的 `image_capabilities` 按 generate/edit 返回参数支持、可用枚举、
数字范围、固定/自定义尺寸和输入输出限制。version=1；缺失/null 为未知，supported=false 明确不支持。
先按用户要求选明确支持的 model_ref，再检查每个 fallback 的独立参数；未知只可正常透传，不能保证支持。
透明任务保留 background=transparent，使用 PNG/WebP；候选不能悄悄删掉透明参数。
工具在生成前检查每个候选的声明，并在实际调用前再次核对；不满足要求返回 not_started。
型号名、描述里的“高清”或编辑布尔标志不能代替尺寸、蒙版和透明能力声明。



图片生成、编辑和处理优先通过 `generate_iyw_image` 使用平台能力或图片模型；工具契约覆盖任务时不要改用 fetch。仅 [电商视频](iyw-api-ecommerce-video.md) 和 [商品套图](iyw-api-product-kits.md) 明确记录的不匹配或未封装操作用 fetch 调原接口，不为绕过校验虚构参数。任务查询、历史、素材、收藏和其他业务仍走 `fetch_iyw_url`，见 [图片任务管理](iyw-api-image-admin.md)。只读本次 type 对应行和示例；核对来源或契约差异时再读 [图片接口证据](iyw-image-api-source.md)。

视频生成优先使用本 Skill 的 `generate_iyw_image`/`fetch_iyw_url`，按 [视频专篇](iyw-api-ecommerce-video.md) 选择。视频导演返回脚本，生成返回任务，成功后才能交付视频；不要把图片模型目录当成视频模型目录。

自定义改款优先 `variation`，单基准图系列延伸优先 `extend`，多图融合优先 `mix`，无需先查模型目录。内置转 3D 已禁用，不调用 `image-to-3d` 或 `commerce/ImageTo3D`，也不通过浏览器或其他封装绕过。

帮客户生图时，适用的平台功能优先；复杂、多重视觉约束或需要更自由控制的任务可直接用
图片模型，不要求先制造平台失败。明确选择某平台模型的要求仍须保留。
普通文生图可用 `micro-generate`，改款／延伸／融合用相应平台 type；选择图片模型后，
无图用 `generate`、有参考图用 `edit`。用明确 type 表达已选路线，不依赖 auto 猜测服务。

这里的“模型一／二／三”分别选择平台 `parameters.modelChannel=1/2/3`，不是 Fusion
模型目录的第一／二／三项。用户明确选择时原样传入，不改成默认模型，不切换 `edit`；
只有未指定通道才省略 modelChannel，沿用默认模型二。`variation/extend/mix` 不接收
`parameters.model`；用户锁定模型时不自行加入其他模型回退。

例如用户要求“自定义改款、模型一、东方中式”，使用：

```json
{"type":"variation","prompt":"保留原图主体，改款为东方中式风格","images":["https://example.com/source.png"],"parameters":{"modelChannel":1}}
```

## 能力匹配与兜底

1. 安装图像库或写处理脚本前，先按用户所需效果、输入与输出选择平台能力或图片模型。没有同名专用接口、关键词未命中或描述简略，都不等于明确不支持；继续评估单图指令编辑或模型目录。自然语言可描述的视觉变化不要求存在同名 API。
2. 核对真实技术约束。`image_generation` / `image_editing` 不保证蒙版、透明输出、位深、参考图数量或任意分辨率；按文档使用参数。不得自行添加精确几何或位深要求来排除服务，也不得丢弃用户要求或调用已知不支持的操作来制造失败。
3. 优先匹配上述平台路线；其他专用操作按实际功能选择。任务复杂、需要更自由控制、用户明确选择图片模型，或优先路线有明确不支持、不可用、确认失败的证据时，选择适用模型；无需先制造平台失败。有参考图且选择目录中的模型时使用 `edit`，无图创作用 `generate`；同任务复用模型目录。把目录引用放入平台操作参数不代表选中了该模型。目录 `model_ref` 仅用于内部调用，对用户只用业务展示名称；名称缺失时说“通用图片处理”。即使追问价格、型号或锁定模型，也不在正文、表格、括号、代码或错误转述中披露货币价格、真实 ID、目录引用、供应商或后端名称。点数仅用平台明确返回的数据，不换算或猜测；未确认时不报数值。
4. 按下表恢复失败，重新对照原始用户目标选择适用服务，不能让辅助步骤的失败决定整个任务改写为本地算法。
5. 仅在适用平台和模型路线均有不可用、文档明确不兼容或有限恢复后确认失败的具体证据时，才可脚本兜底；先说明限制，不遍历无关服务。缺少专用接口、第一张效果不佳或认为本地算法更精确，均不足以回退。

| 当前情况 | 下一步 |
| --- | --- |
| 图片路径或参数错误 | 按 schema 修正输入；带图且选择模型时使用 `edit`，不丢弃参考图 |
| 单个服务明确失败 | 下载/转存 502 由工具自动执行有限同模型重试；其他失败按错误原因恢复，或走已声明候选，不自行无限重放 |
| 超时、网络异常、执行中或提交结果未知 | 查原任务状态；无法确认时保留未知，不重复提交或据此改为本地生成 |
| 已完成图片未满足需验收的效果 | 在任务范围和计费授权内优先用适用服务针对性修正，不无限生成或擅自重建图片 |

本地复制、下载、输入准备和最终排版可以辅助服务调用；重绘、分割、结构推断、高度重映射属于核心图片任务，称作“精修”“确定性处理”或“后处理”也必须遵守上述条件。

## 类型与参数

商品套图/A+ 的完整输入与版本流程见 [套图](iyw-api-product-kits.md)，批量图像任务见 [批量中心](iyw-api-batch-center.md)，枚举按需读 [通道与类型](iyw-image-enums.md)。

`images` 接受 HTTPS URL、工作区本地路径、本轮提供的会话附件路径、file URI、Data URL，或 `{path|url|base64|data,mimeType,role,name}` 对象；原始 Base64 需 MIME。附件直接使用本轮上下文给出的准确路径，不猜测文件名、不遍历目录；主机仅允许本会话受管附件位于工作区外，其他会话附件和任意外部文件仍被拒绝。Fusion 编辑支持最多 16 张 PNG/JPEG/WebP 参考图及额外 1 张 PNG 蒙版，平台操作最多 10 张。本地路径与编码图片先校验真实 MIME，再上传取得 HTTPS URL；已有 URL 保留完整 query 并直接提交，其内容、格式和大小由上游验证。使用编辑蒙版时，主机只读取蒙版与首张参考图以校验 PNG／尺寸。主机读取的单图上限 20 MiB。重试复用同一组 URL。`prompt` 描述目标，`parameters` 放所选操作的业务字段。新原生操作不同时接受顶层 images 和同名 parameters 图片字段。

| type | 后端操作 | 参数与区别 |
| --- | --- | --- |
| `product-kit` / `a-plus` | `product-kit/generate-kit` / `generate-a-plus` | 现有封装需 images + platform/market/language/contentType/resolution/productInfo/modules；A+ 另传 selectedPlan，默认 HTTP 650 秒。新版 smart/custom 套图用 aspectRatio/structure/kitCounts，与旧封装不匹配时走 [套图参考](iyw-api-product-kits.md) 的 fetch 路由 |
| `a-plus-edit` | `commerce/g_tools_generate_image` | 单图 + prompt，固定 A+ 12 类型与变款通道，默认 16:9；生成后按用户要求用 fetch 保存/激活版本 |
| `batch-*` | 10 个平台批量通道的 submit | 具体 type/字段见批量中心；默认 HTTP 120 秒、等待 600 秒；batchId 与普通 taskId 分开 |
| `fission` | `microModel/v2/batch` | 文生图；prompt 必填，按现有 `models/jsonData` 契约，非摘要 items[] |
| `variation` / `extend` | `commerce/g_tools_generate_image` | 单图 + prompt；modelChannel 保留显式选择，省略才默认 2；toolName/batchSize 由主机设置 |
| `mix` | `commerce/g_tools_generate_image` | 2-10 图 + prompt；按输入顺序说明角色；modelChannel 保留显式选择，省略才默认 2 |
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
| `video` | `commerce/videoGenerator` | 现有封装：单图 + prompt；必填 ratio、duration 整数 4-15 秒、mode normal/hd。新页面多图、1-3 秒、product/remake 契约见 [电商视频](iyw-api-ecommerce-video.md)，不可把多图放入 images 后只取首图 |
| `model-scene` / `background` | `commerce/modelScene` | 1-10 图 + prompt；size 比例、resolution standard/4K |
| `modify` | `commerce/imageModification` | 图片 + prompt；可传非负 strength |
| `seed-edit` | `commerce/SeedEdit` | 图片 + prompt，融合创款/指令编辑 |
| `blend` | `commerce/blend` | 2-10 图；prompt 可选，原生融合而非 mix 通道 |
| `erase` / `watermark-erase` | `commerce/erase` / `watermarkEraser` | erase 需 HTTPS mask；watermark-erase 可传 target=text/watermark/text_watermark 或旧 mask；不猜 payOrderNo |
| `extract` / `lineart` | `commerce/extraction` / `lineart` | 图片；lineart 可传 style，区别于旧 line-extraction |
| `vectorize` / `three-views` | `commerce/vectorizeImage` / `threeVisions` | 图片，矢量化/三视图 |
| `bleed-line` | `commerce/bleedLine` | 图片 + size 或补充版 bleed；单位/结构来自实际页面，不能猜毫米/像素 |
| `upscale` / `super-upscale` | `commerce/upscaleImage` / `SuperUpscale` | 图片；可传非负 upscale 和已确认 op，不能提供 token |
| `video-auto-director` / `video-remake-director` | `commerce/videoAutoDirector` / `videoRemakeDirector` | 现有封装仍要求 images + prompt；页面用 userHint，复刻另需 videoUrl；两者只产出 script。页面流程优先按 [电商视频](iyw-api-ecommerce-video.md) 用 fetch，不假设 prompt 自动映射 userHint |
| `extract-color` / `save-color` | `commerce/extractColor` / `saveColor` | 前者图片；后者非空 colors 数组，结构来自提色结果 |
| `detect-grid` / `build-extract-prompts` | `commerce/detectImageGrid` / `buildExtractPrompts` | 图片，返回网格信息或提取提示词 |
| `classify-intent` | `commerce/classifyCanvasIntent` | parameters.text 或旧非空 keys 数组，不要求图片 |
| `background-remove` | `microModel/GetImageSegment` | 单图；主机映射为 imageUrl |
| `check-image` | `microModel/checkImage` | 单图，沿用已确认 image 字段；仅用户要求检查时调用，不自动前置到其他图片操作 |
| `micro-upscale` / `micro-upscale-image` | `microModel/upscale` / `upscaleImage` | 单图映射 imageUrl；amount/device/payMethod/typeId 从实际页面确定 |
| `g-tools` / `f-tools` | `commerce/g_tools` / `f_tools` | 前者 prompt，可选 images/model；后者 content，price/type 来自已确认请求；不猜通道参数 |
| `micro-generate` / `micro-variation` | `microModel/v2/generate` / `variation` | 前者 prompt，可选图片及 modelChannel/ratio/batchSize/tool；后者图片 + prompt |
| `scheme-generate` | `ai-chat/api/designScheme/generateImage` | schemeId + prompt；schemeId 来自设计稿业务列表 |
| `faddish` | `ai-application/faddish/generate` | prompt，可选图片 |
| `generate` / `edit` | Fusion | 无图创作用 generate；edit 需参考图且遵循上文平台优先条件；将当前模型目录的 model_ref 原样放入 parameters.model |

新增原生类型仅验证文档已明确的图片、提示词和基础类型；未提供枚举的字段不硬编码猜测。上游明确拒绝要保留原错误，未知状态先查任务。蒙版服务域已由补充篇明确，但区域字段/坐标结构仍不完整，当前工具不猜测该请求。

## 结果与恢复

### 执行速度与稳定性

- 每批仍最多 64 项、16 路生成请求并发，不设跨批共享的 16 路门槛。输入读取／上传也有独立的 16 路并发，各项准备好即开始提交，不等整批素材。异步任务已创建后的轮询和重试退避不占用生成请求名额，可能同时有超过 16 个已创建任务在服务端运行。
- 同次调用中相同来源和校验方式合并准备，相同内容只上传一次，上传后只保留 URL／校验元数据。不跨调用缓存用户素材或生成结果。Fusion 模型目录由 list 与 generate 共用 60 秒缓存，按账号／工作空间隔离；仅调用时检查时间，过期才请求接口，不运行定时器。并发刷新合并，失败不跨调用缓存；引用缺失时提交前最多刷新一次，仍不支持则明确报错。count、回退和重试复用本次目录快照。
- 全批参数、路线和模型预检在收费调用前完成；单项文件读取／上传失败只影响该项，其他项继续，结果按原顺序汇总。
- 停止会中断排队、输入准备、请求等待、轮询和退避；已提交的远程任务不保证被取消，结果未知时不可重复生成。只读任务查询短暂超时或 429／503 可有限恢复原查询，不重提交生成；查询截止时间按剩余 wait 预算执行。
- 输出中部分图片无法处理时返回 `partial` 并保留成功 URL，细节见 `metadata.output_errors`；已知图片在后续查询为空或失败时仍保留。`unknown` 表示提交结果未确认，不能当作明确失败重提。输入／输出 URL 的签名 query 保留，fragment 不作为输出链接的一部分。

### Fusion 生成／编辑参数

提示词放在顶层 `prompt`，参考图放在 `images`。`parameters.model` 使用目录中的模型引用。
常用字段均已列入工具 schema；生成与编辑都走 JSON，编辑参考图使用 `images[].image_url`，
普通可选字段保留 JSON 类型，编辑省略 null 字段，额外的渠道参数保持透传。是否支持某个选项按所选模型契约判断，
不得把所有模型都当作支持透明、保真或任意尺寸；不固定填充默认质量或尺寸。

| 参数 | 生成／编辑方式 |
| --- | --- |
| `quality` | 支持该参数时使用 low／medium／high／auto；旧版模型可有 standard／hd |
| `size` | auto 或 WIDTHxHEIGHT；灵活大尺寸需当前模型支持，平台工具另按其比例契约 |
| `n` | 单任务上游输出数量 1–10；不能与 count、requests 或 fallbacks 混用 |
| `background` | transparent／opaque／auto；透明要求需原样保留在每个回退候选中 |
| `output_format` | png／jpeg／webp；transparent 只配 png 或 webp |
| `output_compression` | 0–100，只配明确的 jpeg／webp 输出 |
| `response_format` | url／b64_json，只在模型支持此字段时传递，与 output_format 不同 |
| `moderation` | auto／low；编辑是否支持需渠道契约确认 |
| `input_fidelity` | 编辑模型支持时传 high／low；不是所有图片模型都接受的通用参数 |
| `mask` | 编辑 PNG 蒙版的工作区路径、HTTPS URL 或 Data URL；或 images 中提供 role=mask，二选一，尺寸与首张参考图一致 |
| `style` | vivid／natural，仅适用声明支持的生成模型 |
| `user` | 可选匿名业务标识，不包含个人信息或凭据 |
| `stream`／`partial_images` | 当前中转仅完整 JSON；stream 只允许 false／null，partial_images 只允许 0／null，不能宣称支持流式出图 |

单图与多图编辑统一发送 HTTPS URL，蒙版使用 `mask.image_url`；生图请求不发送二进制文件或 Data URL。
上传扩展名与 MIME 来自真实字节，Base64 成图转存同样按 PNG/JPEG/WebP 处理。
模型不支持透明输出时应保留用户要求并报告或选择兼容候选，不把白底成图当作透明 PNG。

高清编辑与透明生成结构示例；须将引用替换为当前目录选中的兼容模型，图片路径位于工作区：

```json
{"type":"edit","prompt":"将图变高清，清晰描绘所有细节，其他不变","images":["assets/source.png"],"parameters":{"model":"MODEL_REF_FROM_CATALOG","quality":"high","size":"auto","output_format":"png"}}
```

```json
{"type":"generate","prompt":"生成完整展示的产品图片，背景透明","parameters":{"model":"TRANSPARENCY_CAPABLE_MODEL_REF","background":"transparent","output_format":"png","quality":"high","size":"1024x1024"}}
```

- `status/images/task_id/metadata/delivery` 为输出。原生操作完整业务结果在 `metadata.result`，色号、网格、提示词、矢量文件或视频链接不能只看 images。只陈述返回证实的操作或模型；Fusion 的 `metadata.model_name` 是业务展示名称，平台操作成功不证明传入的目录模型已执行。
- 平台结果 `metadata.requested_model_channel` 记录实际提交的通道参数，批量与回退轨迹保留；它说明请求选择，不单独证明上游最终使用的内部模型。
- 原生异步任务的 `metadata.query` 给出已知查询路径；恢复查询用 fetch。返回 task_id 但没有已确认查询接口时保留运行中状态，不能宣称成功。
- 用户需要多张独立图片时使用批量 `requests`，最多 64 项、并发 16 项；更多图片拆成多批。每项的 prompt、images、type、parameters、count、wait 和 fallbacks 可完全不同，不能把第一项参数复制覆盖其他项。`count` 每项 1-4，仅用于重复同一意图；所有项 `count × (1 + fallbacks 数量)` 合计最多 128 次计划候选执行，包含回退。下载/转存 502 每候选最多额外重试 3 次，整个批次最多 512 次实际调用，每次可能扣点。批量成功和失败分别报告；数据类操作结果保留在各项 `runs[].metadata.result`。
- 单项可提供最多 3 个按顺序执行的 `fallbacks` 候选，每个包含明确 type、完整 parameters 和可选 wait。候选继承原任务提示词和全部参考图；参数整体替换、不继承前一模型专用字段，不能在 parameters 中覆盖 prompt/来源图，带链请求用 count 而非 n/batchSize。模型候选各自在 parameters.model 使用当前目录引用；不自动选模型、不重复相同 type/parameters。
- 成功、部分成功、取消、运行中、超时、传输异常或查询失败停止链；仅明确终态失败或主机判定可回退的响应进入下一候选。generate/edit 的 HTTP 502 且业务码为 `upstream_image_download_failed` 或 `image_persist_failed` 时，先按 2／4／8 秒间隔重试当前模型，单任务没有 fallbacks 也生效，耗尽后再进入已声明的下一候选。每次重试重新提交生图，前次可能已生成且产生费用，不能说“未生成”或“未扣费”；普通 502 不在此重试范围。鉴权、余额和输入问题先解决，不靠切换模型重放。
- 返回的安全错误细节包含 HTTP 状态、业务错误码、阶段、可用的 request_id 和 execution_status。链的 metadata.attempts 保留各次实际路线、结果、任务编号、耗时和回退/停止原因，包括 candidate_index、retry_index 和 retry_delay_seconds；批量在各项 runs 的 metadata 或 error_details 中保留这些信息。
- 超时/取消/查询失败不重新扣点提交，先查原 task_id。原生查询失败时保留任务 ID 和 poll_error。
- 矢量等文件链接如不在 images 中，通过 metadata.result 读取并按需用 present_task_files 交付。旧 video 封装不保留完整 metadata.result，必要时按 [电商视频](iyw-api-ecommerce-video.md) 用 fetch 查询原始视频 URL；不要声称已在图片预览中展示视频。

```json
{"type":"erase","prompt":"移除选区杂物并延续背景纹理","images":["assets/source.png"],"parameters":{"mask":"https://已上传蒙版的真实地址"}}
```

```json
{"type":"extract-color","images":["assets/palette.png"]}
```

以下是结构示例；图片路径与模型引用须替换为本次真实输入及目录返回值：

```json
{
  "requests": [
    {"id":"front","type":"edit","prompt":"生成参考摆件的正面视图，保持造型和雕饰","images":["assets/source.png"],"parameters":{"model":"MODEL_REF_FROM_CATALOG_A"},"fallbacks":[{"type":"edit","parameters":{"model":"MODEL_REF_FROM_CATALOG_B"}}]},
    {"id":"side","type":"variation","prompt":"生成参考摆件的侧面视图，完整展示耳朵和卷鼻","images":["assets/source.png"],"parameters":{}}
  ]
}
```

## 既有操作与示例

Choose a route whose documented capabilities cover the task:

| Task | Preferred type |
| --- | --- |
| Ordinary text-to-image covered by platform creation | `micro-generate` |
| Complex or freer text-to-image through an image model | `generate` through Fusion `images/generations` |
| Redesign or modify one source image | `variation` |
| Edit source images with a selected catalog model | `edit` through Fusion `images/edits`, preserving source images |
| Fuse 2-10 reference images | `mix`, preserving input order |
| Four-panel grids or same-series extension from one base image | `extend` |

Fusion `edit` (`images/edits`) is suitable for complex edits, freer control or explicit model selection and
requires at least one source image. `generate` and `edit` do not require a prior
platform attempt or failure. `fission` remains an explicitly selectable platform
operation. Select the matching specialized operation for background, outpaint,
super-resolution and similar tasks.

Before `generate`, `auto` without images, or explicit `edit`, call
`list_iyw_image_models` with `{}`. Select from the returned descriptions,
capabilities and task requirements: `generate` needs
`capabilities.image_generation=true`; `edit` needs
`capabilities.image_editing=true`. Copy the exact returned opaque `model_ref` into
`parameters.model`. A missing/stale reference, raw model ID or display name is
rejected; the host does not choose a default model. References remain stable within the current host process, independent of catalog order. Refresh the catalog once after an invalid reference or host restart. Reuse the catalog for the same task or batch;
an empty or failed lookup supplies no model. Platform operations need no Fusion
model lookup. Real IDs, providers and prices are omitted. Replace `MODEL_REF_FROM_CATALOG` in examples before calling.

`auto` uses `generate` without images; with one image it uses `extend` for
series, extension, four-panel or 2x2 wording, and `variation` otherwise; with
multiple images it uses `mix`. Prefer an explicit type when the intent is known.
An `extend` request needs one base image: describe the four-panel layout or series
in its prompt. Without a reference, create the requested composition with
`generate`. Keep multiple references for fusion; for independent series derived
from multiple bases, use separate `requests` items, each with one base image.
Do not discard references to force the single-image extension interface.

Use the returned operation's business name or confirmed model display name in
user-facing explanations, without currency prices, internal IDs, references or backend names. After a timeout,
transport error or uncertain submission, query the original task ID when available;
do not blindly resubmit or switch routes. Deliver ordinary successful images from
returned status, URLs and delivery metadata. Inspect visuals for requested review,
comparison, visual acceptance or integration into a composed deliverable.
Image artifact registration defaults to off. Keep candidate images unregistered
while creating, comparing or selecting them. Set `delivery.registerArtifact` to
`true` only when the image itself is selected as a final file to give the user
in this conversation. For images used in a PPT/report/webpage, keep registration
off and register only the completed deliverable after the task is complete.

**Timeouts:** omit `wait.timeoutSeconds` for 600 seconds on ordinary platform requests and
polling, 650 seconds for product-kit/a-plus HTTP, 120 seconds for batch-center HTTP (polling 600),
or 300 seconds on `generate`/`edit`. The agent can explicitly override
either, including values above 600; prefer the defaults or longer for slow tasks.
Do not shorten waits merely to return sooner. Each batch item has its own wait.
`0` means submit without polling on the platform; Fusion keeps its default timeout.

```json
{"type":"generate","prompt":"白底陶瓷茶壶，现代东方风，产品摄影","parameters":{"model":"MODEL_REF_FROM_CATALOG"},"wait":{"timeoutSeconds":900}}
```

```json
{"type":"variation","prompt":"只把包身改成深绿色防水尼龙，保留版型、拉链、提手和视角","images":["https://example.com/bag.png"]}
```

```json
{"type":"extend","prompt":"保持原图结构和材质语言，延展同系列花瓶","images":[{"url":"https://example.com/vase.png","role":"primary"}],"parameters":{"ratio":"4:3","batchSize":1}}
```

```json
{"type":"mix","prompt":"以第1张产品结构、第2张趋势配色融合成一件可生产餐盘","images":[{"url":"https://example.com/product.png","role":"structure"},{"url":"https://example.com/trend.png","role":"style"}]}
```

When Fusion editing is explicitly selected:

```json
{"type":"edit","prompt":"以原图为参考自由重绘为超现实拼贴海报，重新设计透视和构图，保留主体标识，右侧留出标题区域","images":[{"base64":"...","mimeType":"image/png","role":"source"}],"parameters":{"model":"MODEL_REF_FROM_CATALOG"}}
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
{"type":"video","prompt":"镜头从正面缓慢环绕，展示材质高光","images":["https://example.com/product.png"],"parameters":{"ratio":"16:9","duration":8,"mode":"normal"}}
```

商品套图保留旧工具契约；新版 smart/custom 和爆款复刻按 [套图参考](iyw-api-product-kits.md) 用 fetch。AI 试衣仍缺完整提交契约。出血线、色号提取见上表，其参数与旧页面本地处理设置不能混用。

For a local path use `"images":["assets/product.png"]`; for raw base64 use an
object with `base64` and `mimeType`; for a Data URL pass it as the string source.
For IYW image tools, non-URL sources become public HTTPS before execution;
`edit` uses their validated bytes directly. The decoded input limit is 20 MiB
and HTTP image URLs are rejected.
