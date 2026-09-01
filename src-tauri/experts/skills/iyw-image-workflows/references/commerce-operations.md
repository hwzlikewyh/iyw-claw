# Commerce 图片操作契约

本文只记录已经获得请求样例的 commerce payload。图片生成和变款统一调用服务端
operation `g_tools_generate_image`。变款应使用已封装的本地命令 `tool variation`；
`variation` 只是 CLI 别名和 payload 的 `toolName`，不是独立接口 operation。所有本地图片先通过
`scripts/iyw_commerce.py upload` 上传并检测；已有公开网络图片先通过 `check-image`
检测。当前会话中已经检测成功的 URL 可以直接复用，payload 中只使用检测成功后的公开 HTTPS URL。

## 目录

- 变款
- 系列延伸
- 多图融合
- 智能搜索驱动的趋势或主题设计
- 趋势路由与成组版式
- 本地布局拼接
- 图片放大
- 清单图片工具
- 调用与任务查询

## 变款与首轮极速路径

执行 `tool variation`。CLI 内部调用 `g_tools_generate_image`，并固定设置
`toolName: "variation"`：

```json
{
  "imageUrls": "https://example.iyw/source.png",
  "prompt": "去掉头上的角，其余设计保持不变",
  "toolName": "variation",
  "channelName": "自定义改款",
  "remark": "工具集4o变款",
  "modelChannel": 2,
  "size": "auto",
  "resolution": "standard",
  "batchSize": 1
}
```

`imageUrls` 为一张图片 URL，`prompt` 必须描述要修改的内容。

单张基准图且用户只要一张设计图、改款图或企划案版面图时，直接复用上面的固定 payload：
不要先做趋势搜索、主题拆解、需求解析、企划文档、系列延展或多图融合。标准调用在同一
命令中创建并等待：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  tool variation `
  --input-file "C:\path\payload.json" `
  --wait-seconds 120 `
  --no-progress
```

CLI 会先创建一次收费任务，再轮询同一个 `taskId`；到达等待上限时只返回该任务状态，
后续继续用 `task-wait` 或 `task-get` 查询原 ID，不得创建回退任务。标准模板已由 CLI
固定校验 `toolName=variation`、`modelChannel=2` 和 `batchSize=1`，不再把额外 dry-run
作为首轮用户等待门槛；只有非标准字段或排查请求时才单独 dry-run。

企划案图片也遵循这条路径：用户说“做企划案版面图”时在 `prompt` 中要求一张完整版面图；
用户明确说“写企划文档/分析报告”时才进入文档流程。

## 系列延伸

调用 `g_tools_generate_image`，固定设置 `toolName: "extend"`：

```json
{
  "imageUrls": "https://example.iyw/source.png",
  "prompt": "保持原有视觉语言，设计同系列的新产品",
  "toolName": "extend",
  "channelName": "系列延伸",
  "remark": "工具集4o系列延伸",
  "modelChannel": 2,
  "size": "auto",
  "resolution": "standard",
  "batchSize": 1
}
```

除非用户明确要求无约束延伸，否则不要留空 `prompt`。

## 显式智能搜索驱动的趋势或主题设计

只有用户明确要求使用智能搜索、查趋势、找参考资料或按报告依据设计时，才先完成搜索、
推荐和选择，再创建收费任务。仅提到趋势、主题或企划案时，不进入本节；有基准图的单图
请求仍按首轮极速 variation。推荐和生产流程固定为：

1. 从用户描述提取产品、关键词、市场和时间范围。优先用 `trend-list` 找趋势条目、
   `trend-detail` 取选中趋势的内容，并用 `image` 搜索主题图片；趋势主题图片的
   `classify` 使用 `[51]`。只有来源是报告时才补充报告类搜索。
2. 保持服务端结果顺序，向用户推荐最相关的趋势或主题名称、安全图片和简短内容依据。
   不得把未返回的信息补成搜索结论，也不得把签名 URL、凭据或内部字段放入推荐。
3. 第一次收费生成前，若用户没有说明结果形态，询问一次并等待选择；如果用户已经说
   “出一张设计图/版面图”，不重复询问：
   - 系列作品：同时确认系列包含哪些产品；
   - 4 宫格或 6 宫格：每格一个符合主题的作品，全部格子形成同一系列套组；
   - 单个作品或一个系列并形成企划案：同时确认做单个还是系列。
4. 推荐顺序为系列作品、系列企划案、单个作品；推荐不等于默认授权，不得在用户选择前
   创建收费任务。用户已明确结果形态及产品时不要重复询问。
5. 按选定主题拆解素材页，并使用下表路由。每个主题独立判断页数，不得把不同主题的
   页面误当成同一主题的多页素材。

| 输入状态 | 首个种子任务 | 后续系列任务 |
| --- | --- | --- |
| 已有产品或设计基准图，且只要一张结果 | `tool variation`，搜索内容写入提示词约束 | 用户明确要系列时才改用 `tool extend` |
| 已有产品或设计基准图，明确要同系列/多款 | `tool extend`，搜索内容写入提示词约束 | 继续围绕种子作品做系列延伸 |
| 没有基准图，每个主题一页 | 按主题顺序上传或检测页面，执行 `tool variation` | 用户选择系列时，以种子作品执行 `tool extend` |
| 没有基准图，每个主题多页 | 按页码顺序上传或检测 2 至 10 页，执行一次 `tool mix` | 用户选择系列时，以种子作品执行 `tool extend` |

`variation`、`mix` 和 `extend` 的 payload 都固定使用 `modelChannel: 2`。页面为本地文件时
执行 `upload`，搜索结果已经是公开 HTTPS 图片时执行 `check-image`；任一页面未通过检测
就停止该主题，不得把未检测 URL 放进任务。多页主题超过 10 页时，先让用户选择最多
10 页核心素材，不得自行截断。

4 宫格或 6 宫格分别使用明确的 4 个或 6 个作品位。每格必须对应一个符合主题的作品，
并通过一致的造型语言、配色、材质或图案形成系列套组。企划案除作品图外，还必须包括
趋势或主题依据、主题拆解、产品组合、配色、材质、工艺和系列延展说明。

显式智能搜索请求发生查询失败、响应无结果或没有可用图片时，不得退化为凭空生成；
先让用户选择放宽关键词、市场或时间范围，或者改用用户上传的主题页。

## 趋势路由与成组版式

用户提供一张已上传或已检测的基准图片并只要一张趋势或主题设计图时，调用一次
`tool variation`；只有用户同时明确要求同系列、延展多款或系列方案时才调用 `tool extend`。
创建明确失败、任务明确失败或视觉检查确认结果不满足要求时，才允许按用户目标回退一次；
不得循环重试。创建结果不确定或等待超时时，只查询原 task ID，不得据此创建回退任务。
没有基准图片时，纯文本趋势或主题请求仍使用 `fission-generate`。

`variation` 和 `extend` 的 `batchSize` 固定为 `1`。用户要求宫格、联图、阵列或其他
成组版式时，先在提示词中写明布局、数量和顺序，用一个任务直接生成一张完整合成图，
不要拆成多个并发任务。只有任务明确失败或视觉检查确认布局不符时，才生成各分图并
使用 `compose-layout`。无法视觉检查时保留单任务结果，说明布局未验证，不得增加收费任务。
用户没有明确可计算的行列、图片数量或顺序时，先询问，不得猜测。

趋势或主题成组版式的尝试顺序固定为：先按上文输入状态选择一次完整布局 `extend`、
`variation` 或 `mix`；再按允许的回退执行一次完整布局；最后才生成各分图并本地拼接。
每一步只有前一步明确失败或视觉确认不符时才进入。

## 本地布局拼接

`compose-layout` 只处理本地图片，不读取 token、不访问 API：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  compose-layout --image "C:\path\1.png" --image "C:\path\2.png" `
  --rows 1 --columns 2 --gap 0 --background "#FFFFFF" `
  --out "C:\path\layout.png"
```

按用户指定顺序重复传 `--image`。图片数量必须等于 `--rows` 乘 `--columns`；行列为
正整数，`--gap` 为非负像素。每格使用输入图片中的最大宽高，图片保持比例、居中、
不裁切。输出支持 PNG、JPEG 和 WebP；默认拒绝覆盖，只有用户明确允许时才传 `--force`。

## 多图融合

调用 `g_tools_generate_image`，固定设置 `toolName: "mix"`。`imageUrls` 必须是
包含 2 至 10 个 URL 的数组，并保持用户指定顺序：

```json
{
  "imageUrls": [
    "https://example.iyw/first.png",
    "https://example.iyw/second.png"
  ],
  "prompt": "使用第一张图的产品造型，融合第二张图的篮球主题",
  "toolName": "mix",
  "channelName": "多图融合",
  "remark": "工具集4o多图融合",
  "modelChannel": 2,
  "size": "auto",
  "resolution": "standard",
  "batchSize": 1
}
```

图片少于 2 张或多于 10 张时，在调用接口前直接拒绝。

## 图片放大

调用 `upscaleImage`。`scale` 必须是 1 至 8 的整数：

```json
{
  "image": "https://example.iyw/source.jpg",
  "scale": 2,
  "providerId": 0,
  "width": 1024,
  "height": 1024
}
```

已知原图尺寸时填写 `width` 和 `height`。默认使用 `providerId: 0`；只有用户明确
选择其他 provider 时才传非零值，此时宽高也必须为非零值。

## 清单图片工具

清单工具统一通过 `scripts/iyw_commerce.py tool <alias> --input-file payload.json`
调用。CLI 固定 operation 和 `toolName`，并校验图片 URL、数量与枚举；不得在 payload
中添加 token、Cookie、`securitykey` 或签名 URL。

| 别名 | operation | 关键用途 |
| --- | --- | --- |
| `pattern-apply` | `g_tools_generate_image` | 图案应用 |
| `free-imitation` | `fission` | 自由仿款 |
| `material-product` | `g_tools_generate_image` | 配辅生款 |
| `ip-apply` | `g_tools_generate_image` | IP 应用 |
| `edit` | `erase` | 涂抹编辑 |
| `outpaint` | `outpainting` | 智能扩图 |
| `super-resolution` | `SuperResolution` | 高清修复 |
| `split-layers` | `f_tools` | 元素拆分 |
| `separate-layers` | `g_tools_generate_image` | 元素拆分增强 |
| `enhance` | `EnhanceImage` | 画质增强 |
| `extract-pattern` | `g_tools_generate_image` | 提取图案 |
| `repeat-horizontal` | `g_tools_generate_image` | 二方连续（左右） |
| `convert` | `convert` | 格式转换 |
| `line-extraction` | `lineExtraction` | 导出线稿 |
| `color-transfer` | `g_tools_generate_image` | 配色迁移 |
| `image-to-3d` | `ImageTo3D` | 转 3D 模型 |
| `video` | `videoGenerator` | 视频生成 |
| `model-scene` | `modelScene` | 模特场景图 |

`variation`、`extend`、`mix` 的完整 payload 见上文；其余工具字段以
`功能/补充搜索接口.txt` 中的已确认样例为准，CLI 会拒绝缺少必要字段或非 HTTPS 图片。

## 调用与任务查询

将 payload 写入临时 JSON 文件，再调用一次对应 operation。标准极速 variation 应直接等待：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  tool variation --input-file "C:\path\payload.json" `
  --wait-seconds 120 --no-progress

uv run --project $skillDir --python 3.13 python $commerceCli `
  invoke upscaleImage --input-file "C:\path\payload.json"
```

创建成功后读取 `taskId`，使用同一个 ID 查询：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  task-get --task-id $taskId
uv run --project $skillDir --python 3.13 python $commerceCli `
  task-wait --task-id $taskId --wait-seconds 120
```

不要硬编码价格、模型可用性或未提供的 payload 字段。不要在 payload 中写入 token、
`tokenInfo`、`securityKey` 或签名 URL。
