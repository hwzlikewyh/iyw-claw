# 商品套图、A+、Listing 与版本管理

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

关键词：商品套图、电商套图、Amazon、A+ 详情图、Listing、商品卖点、模块、历史版本。

## 调用归属与工作流

1. `ai-write`、`plan-a-plus`、`listing-copy` 用 fetch_iyw_url，前两项 timeout_seconds:90。ai-write 可仅传 imageUrls；计划返回的 data.plan 原样作为 selectedPlan。
2. `generate-kit` -> generate_iyw_image(type=product-kit)；`generate-a-plus` -> type=a-plus。images 提供商品图，parameters 提供 platform/market/language/contentType/resolution/productInfo/modules，A+ 另传 selectedPlan；默认 HTTP 650 秒。
3. `g_tools_generate_image` 的 A+ 单图编辑 -> type=a-plus-edit；images 固定 1 张，prompt 是完整编辑要求；主机固定 toolName=variation、toolType=12、modelChannel=2、batchSize=1、isChange=1，默认 size=16:9。jsonData/stats 可携带真实来源 groupId/taskId/module 信息。
4. `save-image-version`、历史 list/update/delete 和 searchTaskResult 都用 fetch。只有用户要求保存/激活时传 activate；生成本身不自动修改历史当前版本。

异步套图响应若仍有 taskId，保留原响应 metadata.result，按返回的 task_ids 调用 searchTaskResult，body 的 type 固定为 2；不要把 groupId 当 taskId。A+ 单图编辑由图片工具等待 commerce 任务；源版本保存是之后的独立业务动作。

```json
{"description":"提炼商品卖点","url":"https://gateway.iyw.cn/ai-agent-new/api/product-kit/ai-write","timeout_seconds":90,"body":{"imageUrls":["替换为实际商品图片HTTPS地址"]}}
```

## 来源详细资料

## 二、电商套图 / A+ 详情图 / 商品套图（新挖到，之前文档没有）

这一组是 `ai.iyw.cn` 的「电商智能体」（`agent_id=1100001`）和「商品套图」工作台用的接口，**全部 POST**，路径随部署环境有两种前缀：

| 前缀 | 环境 | 说明 |
| --- | --- | --- |
| `/ai-agent-new/api/product-kit` | 线上（默认） | `VUE_APP_PRODUCT_KIT_API_PREFIX` 未配置时的默认值 |
| `/api/product-kit` | 开发环境（Cookie 带 `agent_new_dev_token`） | 需手动带 `token` + `tokenInfo` 头 |

### 2.1 商品套图 / A+ 详情图（product-kit）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `/ai-agent-new/api/product-kit/ai-write` | 商品卖点提炼 / A+ 文案生成 | `imageUrls[]` 或 `imageUrls` + `platform`、`market`、`language`、`contentType`、`productInfo`、`modules[]` | `imageUrls` 商品图；`platform` 电商平台（Amazon 等）；`market` 国家；`language` 语言；`contentType` 文案类型；`productInfo` 已有卖点（可自动生成）；`modules[]` 要生成的模块 key 列表；超时 90 秒 |
| POST | `/ai-agent-new/api/product-kit/generate-kit` | 生成商品套图（主图+详情图） | `imageUrls[]`、`platform`、`market`、`language`、`contentType`、`resolution`、`productInfo`、`modules[]` | `resolution` 分辨率档位；`modules` 决定出几张/哪几类图；超时 650 秒 |
| POST | `/ai-agent-new/api/product-kit/plan-a-plus` | A+ 详情图方案策划 | `imageUrls[]`、`platform`、`market`、`language`、`contentType`、`productInfo`、`modules[]` | 返回 `data.plan.productSummary` + `modules[]`（每项含 `moduleKey`、`moduleLabel`）；超时 90 秒 |
| POST | `/ai-agent-new/api/product-kit/generate-a-plus` | 生成 A+ 详情图 | `imageUrls[]`、`platform`、`market`、`language`、`contentType`、`resolution`、`productInfo`、`modules[]`、`selectedPlan` | `selectedPlan` 就是 `plan-a-plus` 返回的 plan 对象；超时 650 秒 |
| POST | `/ai-agent-new/api/product-kit/listing-copy` | 生成 Listing 文案（中/英） | 商品信息 + 语言 | 返回 Listing 文案 |
| POST | `/ai-agent-new/api/product-kit/save-image-version` | 保存图片编辑版本 | `toolName`、`groupId`、`taskId`、`versionTaskId`、`activate` | `activate:true` 表示把该版本设为当前版本；`false` 表示仅存档 |
| POST | `/ai-agent-new/api/product-kit/list` | 商品套图历史列表 | 分页 + `dateRange` | 返回 `records[]`，每条含 `groupId`、`taskId`、`imageVersions[]`、`listingCopy` |
| POST | `/ai-agent-new/api/product-kit/update` | 重命名历史记录 | `groupId`、`name` | |
| POST | `/ai-agent-new/api/product-kit/delete` | 删除历史记录 | `groupId` | |
| POST | `/ai-application/api/productKit/history` | 商品套图历史（旧路径） | 分页 | 与 `/list` 同源，环境未配置时的默认前缀 |
| POST | `/ai-application/api/commerce/searchTaskResult` | 批量查任务结果 | `task_ids[]`、`type:2` | `type:2` 固定为商品套图任务类型 |
| POST | `/ai-application/api/commerce/g_tools_generate_image` | 通用生图/改图通道 | 见 §3.2 A+ 单图编辑 | A+ 详情图编辑复用它 |

### 2.2 A+ 详情图单图编辑（走 commerce，不走 product-kit）

实际请求体（源码原文，已完整反解）：

```json
{
  "imageUrls": ["https://.../原图.png"],
  "prompt": "用户输入的编辑要求 + 拼装的模板提示词",
  "toolName": "variation",
  "toolType": 12,
  "modelChannel": 2,
  "size": "16:9",
  "batchSize": 1,
  "isChange": 1,
  "channelName": "A+详情图编辑",
  "remark": "A+详情图单图编辑",
  "stats": {
    "aPlusReedit": true,
    "moduleKey": "main_image",
    "moduleLabel": "主图"
  },
  "jsonData": {
    "schemaVersion": 1,
    "aPlusEdit": {
      "sourceGroupId": "上一轮 groupId",
      "sourceTaskId": "上一轮 taskId",
      "moduleKey": "main_image",
      "moduleLabel": "主图",
      "requirement": "用户原始编辑要求"
    }
  }
}
```

| 字段 | 类型 | 必填 | 作用 |
| --- | --- | --- | --- |
| `imageUrls` | string[] | 是 | 待编辑图片，A+ 场景固定单张 |
| `prompt` | string | 是 | 拼装后的完整提示词（模板 + 用户输入） |
| `toolName` | string | 是 | 固定 `variation`（变款通道） |
| `toolType` | number | 是 | 固定 `12`，A+ 详情图专用类型 |
| `modelChannel` | number | 是 | 出图通道，A+ 固定 `2` |
| `size` | string | 是 | 比例，A+ 单图编辑固定 `16:9`，历史编辑用原图比例 |
| `batchSize` | number | 是 | 出图张数，A+ 固定 1 |
| `isChange` | number | 是 | 是否改款，A+ 固定 1 |
| `channelName` | string | 是 | 渠道中文名，用于历史记录展示（如「A+详情图编辑」「主图编辑」） |
| `remark` | string | 否 | 备注，区分「A+详情图单图编辑」/「A+历史单图编辑」 |
| `stats` | object | 否 | 统计埋点：`aPlusReedit` 标记 A+ 重编辑；`moduleKey/moduleLabel` 标记模块 |
| `jsonData` | object | 否 | 结构化参数：`schemaVersion:1`，`aPlusEdit` 里记录来源 group/task/模块/需求 |

响应里取 `data.taskId`，然后轮询任务详情（process 10=成功、20/30=失败）拿到新图，再用 `save-image-version` 落库。

### 2.3 商品卖点自动生成

调 `/ai-agent-new/api/product-kit/ai-write` 时只传 `{ "imageUrls": [...] }` 即可让 AI 产出卖点，返回 `data.content`（纯文本），前端直接填进 `form.productInfo`。
