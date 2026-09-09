# 图片通道、工具类型、权益与批次枚举

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

这是来源枚举，不是新的 MCP type/capability_id 列表。MCP type 按 [图片工具](iyw-image-tools.md) 或 [套图](iyw-api-product-kits.md)/[批量中心](iyw-api-batch-center.md) 选择。commerce_type、画布 tool、toolName 和 modelChannel 不互换，不能从字符串前缀猜模型 ID。

通道名和可用性以实时配置为准。默认分身旧契约继续由宿主处理；Fusion 模型使用 list_iyw_image_models 真实返回。相同 API 在不同页面的 size/batchSize 支持值可能不同，本表不是全局白名单。

## 来源详细资料

## 六、枚举总表（`commerce_type` / `toolName` / `tool` / `modelChannel` / 尺寸）

这一章把散落在各 chunk 里的枚举值集中起来。只要你知道 `commerce_type` 和 `modelChannel`，就能反推前端展示的中文名。

### 6.1 `commerce_type` 完整映射（38 个，来自 `ai_669`、`ai_3460`）

| `commerce_type` | 中文名 | 说明 |
| --- | --- | --- |
| `background` | 更换背景 | 单图换背景 |
| `multi_background` | 多图背景 | 一次多张背景 |
| `inpaint` | 局部重绘 | 蒙版涂抹重绘 |
| `erase` | 涂抹编辑 | 涂抹消除物体 |
| `text_watermark_eraser` | 消除水印 | 文字/水印擦除 |
| `upscale` | 无损放大 | 与 `super_resolution` 区分，走不同通道 |
| `super_resolution` | 高清修复 | 老图修复；中文名**不带通道后缀** |
| `superscale` | 超级放大 | 高倍放大 |
| `image23d` | 转 3D 模型 | 输出 glb/usdz |
| `convert` | 格式转换 | 格式互转 |
| `segment` | 背景移除 | 抠图 |
| `outpainting` | 智能扩图 | 画布外扩 |
| `seed_edit` | 指令编辑 | 指令式改图（画布的 generate / batch / edit 都归到它） |
| `enhance` | 画质增强 | 清晰度增强 |
| `blend` | 融合创款 | 两张图融合 |
| `lineart` | 线稿渲染 | 画面转线稿 |
| `line_extraction` | 线稿提取 | 提取线条 |
| `gpt-4o-image_draw_single_line` / `gemini-2.5-flash-image_draw_single_line` | 画单线图 | 两个模型版本共用中文名 |
| `gpt-4o-image_extract_pattern` / `gemini-2.5-flash-image_extract_pattern` | 提取图案 | 同上 |
| `gpt-4o-image_variation` / `gemini-2.5-flash-image_variation` | 自定义改款 | 有通道名时显示「自定义改款-通道名」 |
| `gpt-4o-image_mix` / `gemini-2.5-flash-image_mix` | 多图融合 | 有通道名时显示「多图融合-通道名」 |
| `gpt-4o-image_extend` / `gemini-2.5-flash-image_extend` | 系列延伸 | 有通道名时显示「系列延伸-通道名」 |
| `clothingUp` | AI 穿衣 / AI 试衣 | 两处文案不同，同一能力 |
| `mannequin` | 模特与场景图 | 需权益 `I5` |
| `user_product` | 配辅生款 | 配辅料生新图案 |
| `iyw_tu` | 图案应用 | 图案贴到产品 |
| `iyw_ip` | IP 应用 | IP 素材应用 |
| `color_transfer` | 配色迁移 | 迁移配色 |
| `color_extract` | 提取色号 | 输出色值清单 |
| `product_fission` | 自由仿款 | 无需提示词 |
| `three_visions` | 三视图 | 30 点/次 |
| `agent_image_edit` | 图片编辑 | Agent 内联编辑 |
| `return_leftright` | 二方连续（左右） | 连续纹样 |
| `seperate_layers` | 元素拆分 | 分层输出 |

### 6.2 `toolName` 取值（27 个，全量字面量）

`a_plus_detail`、`auto`、`blend`、`color_extract`、`color_transfer`、`comFangle`、`draw_single_line`、`erase`、`extend`、`extract_element`、`extract_pattern`、`generate`、`imgUse`、`iyw_ip`、`iyw_tu`、`lineart`、`mix`、`outpainting`、`product_fission`、`product_kit`、`return_leftright`、`seed_edit`、`seperate_layers`、`user_product`、`variation`、`video`。

其中与页面路由的对应关系：`/comFangle` 配辅生款、`/imgUse` 图案应用、`/ipUse` IP 应用、`/productFission` 自由仿款、`/extend` 系列延伸、`/mix` 多图融合、`/styleVariation` 自定义改款、`/colorPicker` 提取色号、`/colorTransfer` 配色迁移、`/splitLayer` 元素拆分、`/returnLeftright` 二方连续、`/singleLineDraw` 画单线图、`/lineExtraction` 线稿提取、`/removeWaterMark` 消除水印、`/product-kit` 商品套图。

### 6.3 画布 `tool` → 实际提交类型

画布编辑器里的工具名和提交给 `commerce` 的 `toolName` 不完全一致，映射如下：

| 画布 `tool` | 提交 `toolName` |
| --- | --- |
| `paint` | `erase` |
| `mix` | `mix` |
| `extract` | `extract_pattern` |
| `line` | `lineart` |
| `expand` | `outpainting` |
| `variation` | `variation` |
| `watermark` / `generate` / `batch` / `edit` | `seed_edit` |

### 6.4 `modelChannel` 与尺寸

| 参数 | 取值 | 说明 |
| --- | --- | --- |
| `modelChannel` | `0` / `1` / `2` / `3` / `4` | 出图通道 id，实际通道名从 Apollo `ai_gpt_tool_channel` 配置里按 id 查；A+ 详情图编辑固定 `2`；提取图案固定 `2` |
| `size` | `auto` / `1:1` / `3:4` / `4:3` / `16:9` | `auto` 表示由模型自动选比例；A+ 单图编辑固定 `16:9` |
| `batchSize` | `1` / `4` | 一次出图张数 |
| `toolType` | `10` / `12` | `10` = 批量中心提取图案；`12` = A+ 详情图 |
| `platform` | `10` 等数字 | 批量出图固定 `platform:10`；`model_options[].platform` 为通道 id |
| `scale` | `2`~`8` | 批量无损放大倍数，默认 `2` |
| `target` | `text` / `watermark` / `text_watermark` | 批量消除水印的目标类型 |
| `extractionType` | `1` / `2` / `3` | `1` 形状提取、`2` 平铺提取、`3` 透明底提取 |
| `selectedType` | `"1"` / `"2"` / `"3"` | 提取图案的三种模式（与上面对应） |
| `enhanceType` / `model` | `2` / `0` | 批量画质增强固定值 |

### 6.5 批量任务状态与工具 key

| `statusLabel` | 中文 | 前端标签色 |
| --- | --- | --- |
| `pending` | 等待中 | info |
| `running` | 进行中 | warning |
| `completed` | 已完成 | success |
| `partial` | 部分成功 | warning |
| `failed` | 失败 | danger |

批量工具 `toolKey`（用于筛选历史、打包下载）：`shape_fill`、`watermark_eraser`、`series_extend`、`image_enhance`、`image_upscale`、`background_remove`、`extract_pattern`、`replace_scene`、`batch_generate`、`batch_mockup`。
