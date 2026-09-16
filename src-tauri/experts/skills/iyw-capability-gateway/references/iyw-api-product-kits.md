# 商品套图、A+、Listing、爆款复刻与版本管理

先按下表使用已有 `generate_iyw_image` / `fetch_iyw_url`，不创建新 MCP 或 capability_id。视频生成与视频复刻见 [电商视频](iyw-api-ecommerce-video.md)；这里的爆款复刻生成图片。

来源：用户提供的《AI工作台接口梳理（电商视频 / 商品套图）》中 `ai.iyw.cn/#/product-kit?tool=product_kit&view=create` 的 `js/1037.894881fe.js` 具体调用点；结合 2026-09-10 分层补充篇。以下为来源契约与现有工具的静态核对，不表示本次实测。旧摘要与具体字段冲突时见 [冲突表](iyw-api-access-contracts.md)。

## 调用归属

| 功能 | 现有入口 | 参数与边界 |
| --- | --- | --- |
| 新版商品套图 smart/custom | `fetch_iyw_url` -> `generate-kit` | 页面用 aspectRatio/structure/kitCounts；旧 type=product-kit 仍要求 contentType/modules，不能为过校验虚构字段 |
| 已确认符合旧工具契约的套图 | `generate_iyw_image(type=product-kit)` | images + platform/market/language/contentType/resolution/productInfo/modules；仅用于真实已确认的该契约 |
| A+ 方案、卖点、Listing 文案 | `fetch_iyw_url` | 分别 plan-a-plus、ai-write、listing-copy |
| A+ 详情图生成 | `generate_iyw_image(type=a-plus)` | images + 下方业务 parameters；完整 selectedPlan；默认 HTTP 650 秒 |
| 爆款图片复刻 | `fetch_iyw_url` -> `generate-clone` | 目前没有对应图片工具 type；保留参考图与产品图两组输入 |
| A+ 单图编辑 | `generate_iyw_image(type=a-plus-edit)` | 单张 images + 完整 prompt；按来源传 stats/jsonData；默认 16:9 |
| 进度、历史、保存/激活图片版本 | `fetch_iyw_url` | 真实 taskId/groupId；版本保存是独立业务动作 |

现有 a-plus 封装要求 platform/market/language/contentType/resolution/productInfo 为非空字符串、modules 为非空数组和 selectedPlan 为非空对象；商品图按页面最多 6 张，即使工具允许更多也不能扩大。已确认的页面请求若与该校验不匹配，用本篇精确页面 body 经 fetch 调 generate-a-plus，不伪造商品卖点或模块。

全部业务请求为 `POST https://gateway.iyw.cn` + 完整路径、JSON body。fetch 必带动作 description，先读 [HTTP 约定](iyw-http.md)。以下秒数对应 fetch 的 timeout_seconds；图片工具使用 wait.timeoutSeconds。

| 路径前缀 | 用途 |
| --- | --- |
| `/ai-agent-new/api/product-kit/` | generate-kit、listing-copy、ai-write、plan-a-plus、generate-a-plus、generate-clone |
| `/ai-application/api/commerce/` | searchTaskResult、g_tools_generate_image、getCommerceTaskDetail |
| `/ai-application/api/productKit/history/` | list、update、delete、save-image-version |

本地素材用 [通用上传](iyw-upload.md) 取得 URL；专用图片工具可直接接收 images 并上传。商品图页面格式为 png/jpeg/webp，最多 6 张。页面压缩最长边至 2048px、质量 0.9 是前端行为，不代表工具自动处理。

认证由主机承担。来源中 skipDefaultToken 是前端配置，不是业务参数，也不授权手工带 token/tokenInfo。若独立鉴权不适用当前主机登录，说明具体失败；不换开发前缀或读取浏览器凭证。

## 商品套图与文案

`generate-kit`：超时 650 秒，body 如下。其 `contentType/modules` 旧摘要不是本页面请求格式。

| 字段 | 类型、取值与限制 |
| --- | --- |
| `imageUrls` | string[]，实际商品图 URL，最多 6 张 |
| `platform` | string，默认 amazon；amazon/tiktok/temu/shopee/shopify/1688/taobao/jd/pinduoduo/tmall/tmall_global/vipshop |
| `market` | string，us/eu/jp/cn |
| `language` | string，en/zh/ja/ko；与视频导演的中文语言名称不同 |
| `productInfo` | string，商品卖点/要求，最多 500 字 |
| `aspectRatio` | string，默认 1:1；1:1/3:4/4:3/16:9/9:16/auto |
| `resolution` | string，默认 standard；其他档位取服务端 Apollo ai_gpt_tool_channel 中 id=2 的 resolution.subParams，未读取当前选项时不猜 |
| `structure` | string，smart（智能匹配）/custom（自定义） |
| `kitCounts` | object，自定义各类数量 `{whiteBackground,scene,sellingPoint,other}`，总数最多 20；依次为白底/场景/卖点/其他图 |

响应：`data.groupId` 和 `data.items[]`，每项含 taskId/process/error。保留分组与所有子任务，随后批量查询；不能把 groupId 当 taskId。

```json
{
  "description": "生成商品套图",
  "url": "https://gateway.iyw.cn/ai-agent-new/api/product-kit/generate-kit",
  "method": "POST",
  "timeout_seconds": 650,
  "body": {
    "imageUrls": ["https://example.com/product.jpg"],
    "platform": "amazon",
    "market": "us",
    "language": "en",
    "productInfo": "替换为已确认的商品卖点和用户要求",
    "aspectRatio": "1:1",
    "resolution": "standard",
    "structure": "custom",
    "kitCounts": { "whiteBackground": 1, "scene": 2, "sellingPoint": 2, "other": 1 }
  }
}
```

示例只展示字段结构，URL、平台/市场/语言、数量与卖点须匹配实际任务。

| 操作 | 超时 | body | 返回与后续 |
| --- | --- | --- | --- |
| `ai-write` | 90 秒 | `imageUrls:string[]`，最多取前 3 张 | `data.content` 为卖点；按用户要求用于 productInfo，不自动增加未经证实的属性 |
| `listing-copy` | 90 秒 | 与 generate-kit 相同结构，每次 language 只传一个目标语种 | `data.content`；多语种按用户要求逐语种调用并组合，需回写时用 history/update 的 listingCopy 对象 |

文案生成不代表已经生成图片或更新历史。只要求文案时不额外调用生成接口。

## A+ 方案与详情图

`plan-a-plus`：fetch，90 秒。body 为 imageUrls（最多 6 张）、platform（当前 amazon）、market、language、contentType、productInfo（最多 500 字）、modules（最多 10 个模块 key）。market/language 与套图一致；contentType 为 basic（普通 A+）或 premium（高级 A+）。

可选模块：hero 首屏主视觉、selling 核心卖点、usage 使用场景、angles 多角度、detail 商品细节、brand 品牌故事、size 尺寸容量、compare 型号对比、spec 规格参数、craft 工艺制作、package 包装清单、guide 使用建议。

返回 `data.plan`，包含 productSummary 和 modules（含 moduleKey/title/language 等）。保留完整 plan，不凭模块名称手工重建。按用户已授权的方案和模块生成；用户只要求方案时到此停止。

`generate-a-plus`：优先 `generate_iyw_image(type=a-plus)`，images 为商品图；parameters 为 platform/market/language/contentType/resolution/productInfo/modules/selectedPlan。modules 取方案 modules[].moduleKey，selectedPlan 为完整 plan 对象。请求契约不匹配时 fetch 使用相同业务字段、图片放 body.imageUrls，timeout_seconds:650。

返回 `data.groupId` + `data.items[]`（taskId/process/error）。通过图片工具调用时在 metadata.result 读取业务结果；不能因顶层没有 task_id 而忽略 items 中的任务。

## 爆款图片复刻

`generate-clone`：fetch，200 秒。不要把该操作命名为未注册的 generate_iyw_image type。

| 字段 | 类型与取值 |
| --- | --- |
| `cloneType` | string，ecommerce 商品图 / fashion 服饰 / marketing 营销海报 / social 社媒图文 / creative 创意海报 / other 其他 |
| `cloneDegree` | string，style 参考风格（可调整色彩/场景）或 high 高度复刻（替换产品/文案，保留主要布局） |
| `requirement` | string，用户复刻要求 |
| `language` | string，en/zh/ja/ko |
| `aspectRatio` | string，1:1/3:4/4:3/16:9/9:16/auto |
| `referenceImageUrls` | string[]，参考版式/风格图，最多 20 张 |
| `imageUrls` | string[]，待替换的产品图，最多 6 张 |

返回 `data.items[]`（taskId/process）和 `data.stoppedMessage`。保留停止原因与已创建的子任务；部分停止不等于全部失败，也不能忽略已提交任务后整批重发。两组图片角色不同，不合并为通用 images。

## 任务结果与轮询

`POST /ai-application/api/commerce/searchTaskResult`：fetch，30 秒。body 为 `task_ids:string[]`（来自 items[].taskId，转字符串）和 `type:2`。这是 HTTP body 的业务 type，不是图片工具的顶层 type。

来源将业务结果描述为数组，元素含 task_id/taskId、process、images[]（image/cover）、reason；fetch 保留原始 body，若有 code/data 外壳先检查 code=1 再取 data，不假设工具已自动剥离。成功项读取首图 image，缺失时按页面回退 cover。

```json
{
  "description": "查询商品套图生成进度",
  "url": "https://gateway.iyw.cn/ai-application/api/commerce/searchTaskResult",
  "method": "POST",
  "timeout_seconds": 30,
  "body": { "task_ids": ["TASK_ID_FROM_RESPONSE"], "type": 2 }
}
```

process=1/2 为排队/处理中，10 为成功，20/30 为失败。逐项保留对应 URL、reason/error，完整返回后按成功/失败/未完成汇总。未知状态或成功但没有 URL 时不宣称可交付。

手动轮询参考页面每 5 秒、总等待约 720 秒；页面隐藏时 1 秒是前端细节，不要求 Agent 加速。超过等待窗口后保留 IDs 和已完成结果，可继续查历史；不重复生成。HTTP 超时或提交响应丢失先核对原任务/历史，无法确认时报告未知。结果仅部分成功时按实际交付，不用额外的自动重试消耗点数。

## A+ 单图编辑与版本

单图编辑使用 `generate_iyw_image(type=a-plus-edit)`：一张 images，prompt 为完整编辑要求，parameters 可传 size/stats/jsonData。主机请求 `/ai-application/api/commerce/g_tools_generate_image`，固定 toolName=variation、toolType=12、modelChannel=2、batchSize=1、isChange=1；默认 size=16:9、channelName=A+详情图编辑、remark=A+详情图单图编辑。

页面溯源参数：stats 为 `{aPlusReedit:true,moduleKey,moduleLabel}`；jsonData 为 `{schemaVersion:1,aPlusEdit:{sourceGroupId,sourceTaskId,moduleKey,moduleLabel}}`。ID 和模块信息取真实历史/方案记录；旧参考也有 aPlusEdit.requirement，仅在对应契约下使用。历史编辑比例和渠道名按已确认记录，不借其他操作猜值。

生成返回 taskId；专用工具等待 getCommerceTaskDetail，可用 fetch 查询原 taskId 恢复。获得成功新图后，按用户要求保存/激活版本；新图生成不自动改变历史的当前版本。

`POST /ai-application/api/productKit/history/save-image-version`：fetch，30 秒。body 为 toolName、groupId、taskId（源任务）、versionTaskId（新任务），均为 string，及 activate:boolean。true 设置当前版本，false 仅存档；从用户意图确定，不默认激活。返回 `data.imageVersions[]`，检查确实保存了对应版本。

## 历史记录

以下完整路径为 `/ai-application/api/productKit/history/` + 操作，全部 fetch、POST、30 秒。toolName 为 product_kit（套图）、a_plus_detail（A+）、hot_image_replication（爆款复刻）。

| 操作 | body | 结果 |
| --- | --- | --- |
| `list` | toolName:string、page:number、pageSize:number，可选 startDate/endDate:string | `data.total` + `data.list[]`，含 id/groupId/title/images 或 items |
| `update` | toolName:string、groupId:string、title:string 或 listingCopy:object | 重命名用 title；文案回写如 `{zh:"...",en:"..."}`，检查 code/message |
| `delete` | toolName:string、groupIds:string[] | 仅删除用户授权的实际分组，检查 code/message |

生成、改名、回写文案、保存版本和删除是不同动作，只执行当前任务要求的步骤。列表按小分页查询并保留筛选，不把当前页数量当总数。

旧补充篇把历史动作写在 `/ai-agent-new/api/product-kit/{list,update,delete,save-image-version}` 下，并使用 dateRange/name/单个 groupId；这些是历史摘要，当前页面用上表 history 前缀和 startDate/endDate/title/groupIds。不能在失败后换旧路径重放。开发 `/api/product-kit` 也不是生产调用的备用路径。
