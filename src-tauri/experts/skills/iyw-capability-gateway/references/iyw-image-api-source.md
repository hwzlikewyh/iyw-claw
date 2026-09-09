# 图片接口证据与契约差异

这是按需核对的原始接口资料，不是独立调用入口。所有生成/处理操作通过 `generate_iyw_image`；参数和 type 对照先读 [图片工具](iyw-image-tools.md)。任务管理、收藏和素材通过 [图片任务管理](iyw-api-image-admin.md) 使用 `fetch_iyw_url`。

## 证据等级

来源为用户提供的 2026-09-09 文档，来自网站前端包和部分登录态实测；本次未实测扣点接口。通用示例不代表每个接口接受相同字段。已确认的旧工具契约保留，原生新增 type 按本表字段直传，未知必填项由实际页面请求或上游错误确认，不能凭空填默认值。

- `microModel/v2/batch`：现有分身已确认 body 为 `prompt/jsonData/models`，不替换成摘要中的 `items[]`。
- `PreSignedUrl`：现有已确认 `objectKey` -> 带签名 PUT URL；不能改成 `fileName/contentType`。
- `SuperResolution` 旧工具用 `reference/upscale`；`EnhanceImage` 旧工具用 `image/enhanceType/model`；保留。
- `outpainting` 旧工具用 `image/top/right/bottom/left`；`convert` 用 `image/inputFormat/outputFormat`；保留。
- `ImageTo3D` 旧工具用 `image/stats.format/stats.MultiViewImages`；`lineExtraction` 用 `reference/model/batch_size/stats.reference`；保留。
- `g_tools_generate_image` 现有 `variation/extend/mix` 由主机设置 toolName/modelChannel；不与 `g_tools` 通道混用。
- 出血线和提取色号的旧页面可在本地计算；新文档给出 `bleedLine/extractColor` 服务路径，新增 type 显式走该路径，不暗示所有页面已改用接口。
- `/api/generate_mask` 没有明确服务域，商品套图和 AI 试衣没有完整提交契约，均不能猜 endpoint/payload。

## Commerce

完整路径：`https://gateway.iyw.cn/ai-application/api/commerce/...`。

### 3.1 通用入参模式

```json
{
  "imageUrls": ["https://..."],
  "prompt": "描述文本",
  "toolName": "extract_element",
  "channelName": "图层拆分",
  "remark": "画布图层拆分",
  "modelChannel": 2
}
```

**证据**：画布创作实际发送 `{imageUrls:t, prompt:…, toolName:e.toolName, channelName:e.channelName, remark:"画布创作", modelChannel:this.createModelId}`。

| 请求类型 | 接口 | 功能 | 关键入参 | 点数 |
| --- | --- | --- | --- | --- |
| POST | `commerce/fission` | 自由仿款 / 分身生图 | `imageUrls`、`prompt`、`modelChannel`、`batchSize` | 2–4 |
| POST | `commerce/imageModification` | 自定义改款 / 重绘 | `imageUrls`、`prompt`、`strength` | 2–4 |
| POST | `commerce/blend` | 多图融合 | `imageUrls`（多张）、`prompt` | 5 |
| POST | `commerce/SeedEdit` | 融合创款 | `imageUrls`、`prompt` | 3 |
| POST | `commerce/f_tools` | 通用工具通道 | `content`、`price`、`type` | 按模型 |
| POST | `commerce/g_tools` / `g_tools_generate_image` | GPT 类生成 | `prompt`、`images`、`model` | 按模型 |
| POST | `commerce/EnhanceImage` | 画质增强 | `imageUrls` | 2 |
| POST | `commerce/SuperResolution` / `SuperUpscale` / `upscaleImage` | 无损放大 / 超分 | `imageUrls`、`upscale`、`op`、`token` | 2 |
| POST | `commerce/erase` | 涂抹编辑 / 消除 | `imageUrls`、`mask`、`prompt`、`payOrderNo` | 3 |
| POST | `commerce/watermarkEraser` | 消除水印 | `imageUrls`、`mask` | — |
| POST | `commerce/outpainting` | 智能扩图 | `imageUrls`、`ratio`、`prompt` | 2 |
| POST | `commerce/extraction` | 提取图案 | `imageUrls` | 2 |
| POST | `commerce/lineart` | 提取线稿 | `imageUrls`、`style` | 2 |
| POST | `commerce/lineExtraction` | 画单线图 | `imageUrls`、`model`、`stats` | 5 |
| POST | `commerce/extractColor` / `saveColor` | 提取色号 / 保存配色 | `imageUrls`、`colors` | — |
| POST | `commerce/bleedLine` | 出血线工具 | `imageUrls`、`size` | 1/导出 |
| POST | `commerce/convert` | 格式转换 | `imageUrls`、`format` | 5 |
| POST | `commerce/vectorizeImage` | 矢量化 | `imageUrls` | — |
| POST | `commerce/ImageTo3D` | 转 3D 模型 | `imageUrls`、`format`、`MultiViewImages` | 30 |
| POST | `commerce/threeVisions` | 转三视图 | `imageUrls` | 5 |
| POST | `commerce/modelScene` | 模特场景图 | `imageUrls`、`scene`、`ratio` | 5 |
| POST | `commerce/videoGenerator` / `videoAutoDirector` / `videoRemakeDirector` | 图转视频 / 电商视频 | `imageUrls`、`prompt`、`ratio`、`duration`、`mode` | 10 |
| POST | `commerce/detectImageGrid` | 九宫格检测 | `imageUrls` | — |
| POST | `commerce/classifyCanvasIntent` | 画布意图识别 | `keys` | — |
| POST | `commerce/buildExtractPrompts` | 构建提取提示词 | `imageUrls` | — |
| POST | `commerce/getCommerceTasks` | 任务列表 | `page`、`pageSize`、`status` | 查询 |
| POST | `commerce/getCommerceTaskDetail` | 任务详情 | `taskId` | 查询 |
| POST | `commerce/getCommerceTaskRecycleList` | 回收站 | 分页 | 查询 |
| POST | `commerce/removeTaskOrImage` | 删除任务/图片 | `taskId`、`imageId` | — |
| POST | `commerce/addCommerceCollect` / `removeCommerceCollect` | 收藏 / 取消 | `imageId` | — |

## MicroModel

完整路径：`https://gateway.iyw.cn/ai-application/api/microModel/...`；末行 `promptCase` 与 `microModel` 同级。

### 3.2 模型生成（`microModel`）

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `microModel/v2/generate` | 提交生成任务（核心） | `prompt`、`imageUrls`、`modelChannel`、`ratio`、`batchSize`、`tool` |
| POST | `microModel/v2/batch` | 批量生成 | `items[]` |
| POST | `microModel/variation` | 变体生成 | `prompt`、`imageUrls` |
| POST | `microModel/GetList` | 生成记录 | `page`、`pageSize`、`state` |
| POST | `microModel/GetDetails` | 任务详情 | `taskId` |
| POST | `microModel/Recycle` | 回收站 | 分页 |
| POST | `microModel/task/detele` | 删除任务 | `taskId`（接口名拼写为 detele） |
| POST | `microModel/upscale` / `upscaleImage` | 放大 | `imageUrl`、`amount`、`device`、`payMethod`、`typeId` |
| POST | `microModel/GetImageSegment` | 图像分割 | `imageUrl` |
| POST | `microModel/chat/history` | 对话历史 | `chatId`、分页 |
| POST | `microModel/createNewChat` | 新建会话 | `state` |
| POST | `microModel/reset` | 重置会话 | `chatId` |
| POST | `microModel/collects` / `userCollections` | 收藏列表 | 分页、`type` |
| POST | `microModel/addCollect` / `removeCollect` / `deleteCollectImg` | 收藏增删 | `imageId` |
| POST | `microModel/checkImage` | 图片合规检测 | `imageUrl` |
| POST | `microModel/PreSignedUrl` | 上传预签名 | `fileName`、`contentType` |
| GET | `microModel/stsToken` | STS 临时凭证 | 无 |
| POST | `microModel/getMateial` | 取素材 | `materialId` |
| POST | `microModel/RequirementImageDetail` | 需求图详情 | `imageId` |
| POST | `promptCase/getAllCase` | 提示词案例库 | 无（实测 143 KB） |

## 费用参考

## 十一、点数消耗规则（实测自点数管理页）

**AI 设计（按分身）**：分身一 2 点、分身二 4 点、分身三 3 点、分身四 3 点、分身五 2 点、分身六 2 点；重绘同价。垂直模型 / 私有模型 1 点×图片数量。

**工具集**：

| 操作 | 点数 | 操作 | 点数 |
| --- | --- | --- | --- |
| 更换背景 | 5 | 提取线稿 | 2 |
| 多图背景 | 5 | 无损放大 | 2 |
| AI 试衣 | 5 | 涂抹编辑 | 3 |
| 模特场景图 | 5 | 转 3D 模型 | 30 |
| 三视图 | 5 | 格式转换 | 5 |
| 背景移除 | 2 | 出血线工具 | 1/次导出 |
| 图转视频 | 10 | 提取图案 | 2 |
| 画单线图 | 5 | 画质增强 | 2 |
| 智能扩图 | 2 | 融合创款 | 3 |
| 线稿渲染 | 2 | 自定义改款 | 5 |
| 多图融合 | 5 | 自由仿款 | 2 |
| 系列延伸 | 5 | 配辅生款 | 5 |
| 图案应用 | 5 | | |


以上点数是原采集时的观察值，不硬编码为当前报价；需要价格时用 fetch 查实时权益/点数配置。一次 `count` 表示一次独立执行，可能分别扣点。
