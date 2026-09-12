# 瓶型瓶盖与 Temu 商品

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

本表 `frogPrince/` 补 `/ai-agent/api/` 前缀。Temu `fetchGoodsDetail?goodsId={id}` 中 `goodsId` 实为 URL 查询参数，放 `query`，不是路径段，也不是 body。

检索词：瓶型、瓶盖、包装、瓶身、瓶颈、材质、上下架、frogPrince、Temu、商品详情、区域。

流程：查询瓶型/瓶盖 -> 详情 -> 按用户要求更新或上下架。新增/更新“瓶型字段”“盖字段”未给完整定义，从当前页面确认，不能只传 ID 就声称更新成功。Temu goodsId 应来自用户或上游记录。

```json
{"description":"查询 Temu 区域","url":"https://gateway.iyw.cn/ai-agent/api/temu/fetchRegionIds","method":"GET"}
```

## 接口与参数

## 七、瓶型与瓶盖工具（`ai-agent/api/frogPrince`）

包装（瓶型/瓶盖）设计辅助，**全部 POST**。

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `frogPrince/getBottleList` | 瓶型列表 | 分页、筛选 |
| POST | `frogPrince/getBottleDetail` | 瓶型详情 | `bottleId` |
| POST | `frogPrince/addBottle` | 新增瓶型 | 瓶型字段 |
| POST | `frogPrince/updateBottle` | 更新瓶型 | `bottleId` + 字段 |
| POST | `frogPrince/deleteBottle` | 删除瓶型 | `bottleId` |
| POST | `frogPrince/setBottleShelf` | 瓶型上下架 | `bottleId`、`shelf` |
| POST | `frogPrince/getBottleMaterial` | 瓶身材质 | 无/筛选 |
| POST | `frogPrince/getBottleNeck` / `searchBottleNeck` | 瓶颈款式 / 搜索 | 关键词 |
| POST | `frogPrince/getCapList` / `getCapDetail` | 瓶盖列表 / 详情 | 分页 / `capId` |
| POST | `frogPrince/addCap` / `updateCap` / `deleteCap` | 瓶盖增改删 | `capId` + 字段 |
| POST | `frogPrince/setCapShelf` | 瓶盖上下架 | `capId`、`shelf` |
| POST | `frogPrince/getCapMaterial` / `getCapNeck` / `searchCapNeck` | 盖材质 / 盖口 / 搜索 | 关键词 |

---

## 八、电商平台对接（`ai-agent/api/temu`）

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| GET | `ai-agent/api/temu/fetchRegionIds` | 拉取 Temu 区域 ID | 无 |
| GET | `ai-agent/api/temu/fetchGoodsDetail?goodsId={id}` | Temu 商品详情 | `goodsId`（**路径参数**，非 body） |


