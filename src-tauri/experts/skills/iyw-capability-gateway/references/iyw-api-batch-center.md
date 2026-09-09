# 批量图片中心与结果打包

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

提交图片处理必须用 generate_iyw_image 的以下 type；status、history/list/detail/delete、packDownload 均用 fetch_iyw_url。

| MCP type | 批量服务 | 关键 parameters |
| --- | --- | --- |
| batch-shape-fill | batchShapeFill | shape/pattern/scale/quality/size/modelChannel，按所选形状模式 |
| batch-generate | batchGenerate | prompt、modelChannel、size |
| batch-watermark | batchWatermark | target=text/watermark/text_watermark |
| batch-series-extend | batchSeriesExtend | prompt、modelChannel |
| batch-enhance | batchEnhance | 默认 enhanceType:2、model:0 |
| batch-upscale | batchUpscale | scale 2-8，默认 2 |
| batch-background-remove | batchBackgroundRemove | 无额外必填 |
| batch-extract-pattern | batchExtractPattern | 可选 prompt/quality/size/resolution |
| batch-replace-scene | batchReplaceScene | scene、productImageUrls、prompt |
| batch-mockup | batchMockup | 真实 mockupId、可选 prompt |

images 为本次 1-10 张输入；parameters.batchSize 是每张输入的出图数，默认 1；顶层 count 是重复执行整个批次，会额外收费，两者不能同时提供。MCP requests 数组与平台批量中心是不同层级，批量中心使用单次请求形式即可。

提交 HTTP 默认 120 秒；主机以 batchId 轮询本工具的 /status，默认最多等待 600 秒，wait.timeoutSeconds 可控制等待/请求，0 只提交。metadata.batch_id 保存批次 ID，metadata.query 是状态路径。pending/running/completed/partial/failed 分别归一到 queued/running/succeeded/partial/failed；partial 是终态。查询失败保留批次，不重新提交。

```json
{"type":"batch-watermark","images":["assets/a.png","assets/b.png"],"parameters":{"target":"text_watermark"}}
```

打包下载 timeout_seconds:600 仍受 fetch 2 MiB 响应上限约束。超过上限时不能循环重试；应使用接口提供的可下载 URL，若仅提供大型二进制则当前 fetch 无法完成该下载，明确说明。

## 来源详细资料

## 三、批量中心（Batch Center，新挖到）

入口路由：`/manageCenter/batchCenter`，包含 10 个批量工具。**全部 POST，全部走 `https://gateway.iyw.cn`**，路径由环境变量控制，默认值如下：

| 批量工具 | 接口前缀 | submit | status |
| --- | --- | --- | --- |
| 形状填充 | `/ai-application/api/batchShapeFill` | POST `/submit` | POST `/status` |
| 批量生图 | `/ai-application/api/batchGenerate` | POST `/submit` | POST `/status` |
| 消除水印 | `/ai-application/api/batchWatermark` | POST `/submit` | POST `/status` |
| 系列延伸 | `/ai-application/api/batchSeriesExtend` | POST `/submit` | POST `/status` |
| 画质增强 | `/ai-application/api/batchEnhance` | POST `/submit` | POST `/status` |
| 无损放大 | `/ai-application/api/batchUpscale` | POST `/submit` | POST `/status` |
| 背景移除 | `/ai-application/api/batchBackgroundRemove` | POST `/submit` | POST `/status` |
| 提取图案 | `/ai-application/api/batchExtractPattern` | POST `/submit` | POST `/status` |
| 替换场景 | `/ai-application/api/batchReplaceScene` | POST `/submit` | POST `/status` |
| 样机（Mockup） | `/ai-application/api/batchMockup` | POST `/submit` | POST `/status` |
| 通用批量中心 | `/ai-application/api/batchCenter` | POST `/history/list`、`/history/detail`、`/history/delete`、`/packDownload` | — |

### 3.1 通用入参（submit）

所有批量工具提交共用一层结构：

```json
{
  "imageUrls": ["https://.../1.png", "https://.../2.png"],
  "batchSize": 1
}
```

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `imageUrls` | string[] | 要批量处理的图片，上限由各工具页面控制 |
| `batchSize` | number | 每张图产出数量，默认 1 |
| 其他工具专属字段 | — | 见下表 |

### 3.2 各工具专属字段

| 工具 | 专属字段 | 作用 |
| --- | --- | --- |
| 形状填充 `batchShapeFill` | `shape`、`pattern`、`scale`、`quality`、`size`、`modelChannel` | `shape` 形状（支持 `knife` 刀模、`pattern` 图案两种模式）；`scale` 缩放；`quality` 画质档；`size` 尺寸 |
| 批量生图 `batchGenerate` | `prompt`、`modelChannel`、`size`、`batchSize` | 走所选私有模型计费 |
| 消除水印 `batchWatermark` | `target` | 取值 `text`（消除文字）/ `watermark`（消除水印）/ `text_watermark`（两者都消） |
| 系列延伸 `batchSeriesExtend` | `prompt`、`modelChannel` | 每个任务按模型计费 |
| 画质增强 `batchEnhance` | `enhanceType`、`model` | `enhanceType` 增强类型；`model` 指定模型 |
| 无损放大 `batchUpscale` | `scale` | 放大倍数 |
| 背景移除 `batchBackgroundRemove` | 无 | 只需 `imageUrls` |
| 提取图案 `batchExtractPattern` | `prompt`、`quality`、`size`、`resolution` | 默认提示词已内置（保持图案形状/线条/颜色不变，去除产品本体） |
| 替换场景 `batchReplaceScene` | `scene`、`productImageUrls`、`prompt` | 产品图 + 场景图 |
| 样机 `batchMockup` | `mockupId`、`prompt` 等 | 样机模板 |

### 3.3 任务与历史

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `.../history/list` | 批量任务历史列表 | 分页 |
| POST | `.../history/detail` | 批量任务详情 | `batchId` |
| POST | `.../history/delete` | 删除批量任务 | `batchId` |
| POST | `.../packDownload` | 打包下载结果（ZIP） | `batchId`、`toolKey`；`responseType:"blob"`，超时 600 秒；文件名从 `content-disposition` 取 |
| POST | `.../submit` | 提交批量任务 | 见 3.1/3.2，超时 120 秒 |
| POST | `.../status` | 查询批量进度 | `batchId`，返回 `statusLabel`、`totalCount`、`progressFinished`、`progressPercent`、`successCount`、`resultImages[]` |

**返回结构（submit/status 共有）**：

```json
{
  "code": 1,
  "data": {
    "batchId": "xxx",
    "statusLabel": "pending",
    "totalCount": 10,
    "progressFinished": 0,
    "progressPercent": 0,
    "successCount": 0,
    "resultImages": []
  }
}
```
