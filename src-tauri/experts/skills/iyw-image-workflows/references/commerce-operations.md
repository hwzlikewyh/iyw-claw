# Commerce 图片操作契约

本文只记录已经获得请求样例的 commerce payload。图片生成和变款统一调用服务端
operation `g_tools_generate_image`。变款应使用已封装的本地命令 `tool variation`；
`variation` 只是 CLI 别名和 payload 的 `toolName`，不是独立接口 operation。所有本地图片先通过
`scripts/iyw_commerce.py upload` 上传并检测；已有网络图片先通过 `check-image`
检测。payload 中只使用检测成功后返回的公开 HTTPS URL。

## 目录

- 变款
- 系列延伸
- 多图融合
- 图片放大
- 清单图片工具
- 调用与任务查询

## 变款

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

将 payload 写入临时 JSON 文件，再调用一次对应 operation：

```powershell
uv run --project $skillDir --python 3.13 python $commerceCli `
  tool variation --input-file $payloadPath
uv run --project $skillDir --python 3.13 python $commerceCli `
  invoke upscaleImage --input-file $payloadPath
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
