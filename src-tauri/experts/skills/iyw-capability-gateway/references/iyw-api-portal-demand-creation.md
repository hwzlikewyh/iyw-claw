# 设计云需求、比稿、作品与创作者

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：本域所有表格路径加在 `https://www.iyw.cn/gateway` 后。主机自动注入当前登录态对应的 `iyuanwu_token` Cookie；不要提供 Cookie/token 参数。认证失败时恢复主机登录，不从浏览器提取凭证。

检索词：需求大厅、比稿、报名、投稿、稿件、作品分类、设计师、入驻、签约、发布需求、Demand、Creation、Designer。

流程：查询需求 -> 分别读取基础信息和描述 -> 按需读取稿件。`demandId`、`creationId`、`designerId` 各有归属，不能互换。发布前校验仅用于已授权的发布流程；不自动报名、发布、提现。`Subit` 等原路径拼写不做纠正。只写“分页”“表单”的行需补齐当前页面的确切字段。

```json
{"description":"查询设计云需求列表","url":"https://www.iyw.cn/gateway/Demand/GetDemand","body":{"page":1,"pageSize":20}}
```

## 接口与参数

### 10.1 需求与比稿

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `/Demand/GetDemand` | 需求列表 | `page`、`pageSize`、`type` |
| POST | `/Demand/GetMyPublishDemand` | 我发布的需求 | 分页 |
| POST | `/Demand/GetMyCollectionDemand` | 我收藏的需求 | 分页 |
| POST | `/Demand/GetCenterDemand` | 需求中心 | 分页、筛选 |
| POST | `/Demand/GetAdoptDemand` | 已采纳需求 | 分页 |
| POST | `/Demand/GetSignUpDemand` | 已报名需求 | 分页 |
| POST | `/Demand/GetMyDemandCreationList` | 需求下我的稿件 | `demandId` |
| POST | `/Demand/GetDemandDetailBase` | 需求基础详情 | `demandId` |
| POST | `/Demand/GetDemandDetailDescription` | 需求描述详情 | `demandId` |
| POST | `/Demand/GetDemandManuscriptList` | 需求稿件列表 | `demandId`、分页 |
| POST | `/Demand/Publish` | 发布需求 | `title`、`content`、`budget`、`deadline` |
| POST | `/Demand/DemandEdit` | 编辑需求 | `demandId` + 字段 |
| POST | `/Demand/ApplyDemand` | 报名需求 | `demandId` |
| POST | `/Demand/CollectDemand` | 收藏需求 | `demandId` |
| POST | `/Demand/CloseDemand` | 关闭需求 | `demandId` |
| POST | `/Demand/Refuse` | 驳回 | `demandId`、`reason` |
| POST | `/Demand/CheckPublish` | 发布前校验 | 表单 |

### 10.2 作品与创作者

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `/Creation/GetMyCreations` | 我的作品 | 分页、筛选 |
| POST | `/Creation/GetMyCollectCreations` | 我收藏的作品 | 分页 |
| POST | `/Creation/GetDetail` | 作品详情 | `creationId` |
| POST | `/Creation/GetDetailByEdit` | 编辑态详情 | `creationId` |
| POST | `/Creation/GetHotTags` | 热门标签 | 无 |
| POST | `/Creation/GetSearchRecommend` | 搜索推荐 | `keywords` |
| POST | `/Creation/GetCreationsChannel` | 作品渠道 | `creationId` |
| POST | `/Creation/GetChannelsEdit` | 渠道编辑数据 | `creationId` |
| POST | `/Creation/GetCustomClassify` | 自定义分类 | 无 |
| POST | `/Creation/InsertUpdateCustomClassify` | 增改自定义分类 | `id`、`name` |
| POST | `/Creation/SortCustomClassify` | 分类排序 | `ids[]` |
| POST | `/Creation/DeleteCustomClassify` | 删除分类 | `id` |
| POST | `/Creation/SetCreationClassify` | 作品归类 | `creationId`、`classifyIds[]` |
| POST | `/Creation/GetSubmitCreationPic` | 投稿图片 | `creationId` |
| POST | `/Creation/PerfectCreationPic` | 完善作品图 | `creationId` |
| POST | `/Creation/GetCreationMarketTag` | 作品市场标签 | `creationId` |
| POST | `/Creation/GetOrderCreationBySaas` | SaaS 侧作品 | 分页 |
| POST | `/Creation/Tagboxs` | 标签盒 | 无 |
| POST | `/Creation/GetWXCode` | 小程序码 | `creationId` |
| POST | `/Creation/UpdateSourceMaterialPrice` | 改原素材价格 | `creationId`、`price` |
| POST | `/Designer/GetDesignerInfo` | 设计师信息 | `designerId` |
| POST | `/designer/settled/getStatus` | 入驻状态 | 无（未登录 403） ✅ |
| POST | `/designer/settled/addOrUpdate` | 入驻申请 | 表单 |
| POST | `/designer/settled/completeCert` | 完成认证 | 表单 |
| POST | `/designer/settled/getRecordDetail` | 入驻记录详情 | `id` |
| POST | `/designer/settled/getSignContractInfo` | 签约信息 | 无 |
| POST | `/designer/settled/queryContract` | 查合同 | `contractId` |
| POST | `/designer/settled/withdraw` | 设计师提现 | `amount` |
