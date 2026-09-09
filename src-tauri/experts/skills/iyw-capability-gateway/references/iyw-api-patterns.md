# 图案、IP、概念稿、民艺与授权

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

检索词：正版图案、作品库、热门类目、热门作品、主题标签、IP、概念稿、民艺、展会报告、订单、类目授权、系列、收藏。

流程：类目/列表 -> 真实作品或 IP ID -> 详情或关联作品 -> 用户授权的收藏/发布/订单操作。`Ip` 与 `ip` 路径大小写保留原文；`GetHomeWallList.pageSize` 不超过 100。分页字段只写“分页”的接口没有完整 schema，不从其他接口借用字段。`/exhibition/report/*` 的服务前缀原资料未确认，先从当前页面确定完整 URL，不能直接假设在 gateway 根路径。

```json
{"description":"查询正版图案作品","url":"https://gateway.iyw.cn/tu-zp/api/Creation/GetCreationList","body":{"page":1,"pageSize":20,"keywords":"植物"}}
```

购买、续期、授权和意向提交均可能产生业务承诺；依据用户当前任务执行，查询不自动附带这些操作。

## 接口与参数

## 五、图案网（`tu-zp`）

### 5.1 作品与类目

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `tu-zp/api/Creation/GetCreationList` | 作品列表（核心） | `page`、`pageSize`、`classify[]`、`keywords`、`orderBy`、`market` | `classify` 类目数组；`orderBy` 排序；`market` 内外销 ✅ 实测 158564 条 |
| POST | `tu-zp/api/Creation/GetSearchCreationList` | 作品搜索 | 同上 + `keywords` | |
| POST | `tu-zp/api/Creation/GetCreationArea` | 按地区取作品 | `areaId`、分页 | |
| POST | `tu-zp/api/Creation/GetEmpoweredList` | 已授权作品 | 分页 | |
| POST | `tu-zp/api/Creation/GetHomeWallList` | 首页作品墙 | `page`、`pageSize` | ⚠️ `pageSize>100` 报错 ✅ |
| POST | `tu-zp/api/Creation/PublishCreation` | 发布作品 | `title`、`cover`、`classify`、`price`、`auth[]` | |
| POST | `tu-zp/api/CreationSeries/GetCreationSeriesList` | 系列作品 | `creationId` | 缺参报「参数错误」 ✅ |
| POST | `tu-zp/api/HotCreation/GetList` | 热门作品 | 分页 | ✅ 实测 141 条 |
| POST | `tu-zp/api/HotCreation/GetHotCreationInfo` | 热门详情 | `creationId` | |
| POST | `tu-zp/api/Classification/GetPopular` | 热门类目 | 无 | ✅ 返回 `[{classificationId,name,level,parentIds}]` |
| POST | `tu-zp/api/Classification/GetEmpowerList` | 可授权类目 | 无 | |

### 5.2 主题 / IP / 概念稿 / 西南民艺

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `tu-zp/api/Theme/GetRecommendThemeList` | 推荐主题 | 分页 ✅ 实测 1391 条 |
| POST | `tu-zp/api/Theme/GetThemeTagTree` | 主题标签树 | `tagId`（无参报错） ✅ |
| POST | `tu-zp/api/Theme/GetThemeIsHot` | 主题是否热门 | `themeId`（无参报「主题不存在」） ✅ |
| POST | `tu-zp/api/Ip/GetList` | IP 列表 | 分页 ✅ 138 条 |
| POST | `tu-zp/api/Ip/GetIpDetails` | IP 详情 | `ipId` ✅ 缺失报「IP信息不存在」 |
| POST | `tu-zp/api/Ip/GetIpCreationList` | IP 下作品 | `ipId`、分页 ✅ 实测 1036 条 |
| POST | `tu-zp/api/ip/GetDesignPatternList` | IP 图案列表 | `ipId`、分页 |
| POST | `tu-zp/api/ConceptDraft/GetList` | 概念稿列表 | 分页 |
| POST | `tu-zp/api/ConceptDraft/GetConceptDraftDetails` | 概念稿详情 | `id` |
| POST | `tu-zp/api/ConceptDraftSalesRecord/BuyerConceptDraft` | 概念稿销售记录 | `id` |
| POST | `tu-zp/api/ConceptDraftCollection/SaveOrCancelCollect` | 概念稿收藏 | `id`、`type` |
| POST | `tu-zp/api/SouthwestCreation/GetIndex` | 西南民艺首页 | 无 ✅ 实测返回含版权登记号 |
| POST | `tu-zp/api/SouthwestCreation/GetList` / `GetDetails` / `GetTagList` | 民艺列表/详情/标签 | 分页 / `id` |
| POST | `tu-zp/api/SouthwestCreation/Likes` | 点赞 | `id` |
| POST | `tu-zp/api/SouthwestCreation/SubmitIntentionInfo` | 提交意向 | 表单字段 |
| POST | `tu-zp/api/tag/getByClassifyPage` | 类目标签分页 | `classifyId`、分页（未登录 403） ✅ |
| POST | `/exhibition/report/detail` | 展会报告详情 | `reportId` |
| POST | `/exhibition/report/queryList` / `getAreaList` | 展会报告列表/地区 | 分页 |

### 5.3 订单与授权

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `tu-zp/api/Order/Submit` | 提交订单 | `creationId`、`categoryIds[]`、`remark` |
| POST | `tu-zp/api/Order/Cancel` | 取消订单 | `orderCode` |
| POST | `tu-zp/api/Order/GetOrderCodeByCreationId` | 按作品取订单码 | `creationId` |
| POST | `tu-zp/api/Order/GetRenewalEmpowerList` | 续期授权列表 | 分页 |
| POST | `tu-zp/api/Order/GetCategoryActuality` | 类目可售状态 | `categoryId` |
| POST | `tu-zp/api/Empower/EmpowerCategory` | 按类目授权 | `creationId`、`categoryIds[]` |
| POST | `tu-zp/api/Empower/EmpowerSourceMaterial` | 授权原素材 | `creationId` |
| POST | `tu-zp/api/Empower/GetClassifyIdListByCreationId` | 取可授权类目 | `creationId` |
| POST | `tu-zp/api/Series/GetSeriesList` / `AddOrEditSeries` / `DelSeries` | 系列管理 | 分页 / 系列体 / `seriesId` |
| POST | `tu-zp/api/CollectionCenter/AddOrCancelCollect` | 收藏 | `targetId`、`type` |
