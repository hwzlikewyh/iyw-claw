# 消息、字典、设计服务与其他补充

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

统一 fetch_iyw_url。按表中明确域名和前缀；/account-search/basic/dict/getByKey 使用 https://gateway-internal.iyw.cn，key 放 query。SCM 的 /be、/basic/city 等需要真实生产 VUE_APP_PRODUCT_URL，不能拿演示地址推断。设计侧 /api/HandleTags 也需核对实际 origin，tuapi.chdesign.cn 超出本工具域名边界。

通配符 *、“等5个”以及别名简称只代表索引，不能作为 URL 原样提交。客户需求/设计稿/图案/瓶型等精确路径同时查原领域参考。补充把 faddish 称“语言生成”，原资料称造花生图，保留既有图片类型，仅在目标操作实际确认时使用。

## 来源详细资料

### 9.9 其余零散接口

| 请求类型 | 接口 | 功能 | 关键入参 |
| --- | --- | --- | --- |
| POST | `/message-service/sendRecord/pageList` | 消息发送记录 | 分页 |
| POST | `/message-service/sendRecord/recordRead` | 标记已读 | `id` |
| POST | `/messagecenter/Message/RecvMsgParser` | 接收消息解析 | 消息体 |
| POST | `/messagecenter/Message/SendWeComGroupMsg` | 发企微群消息 | 群 + 内容 |
| POST | `/be/getProductConfigure` | 取产品配置（走 SCM） | — |
| POST | `/be/setProductConfigure` | 存产品配置（走 SCM） | 配置 |
| POST | `/contract/base/accountRegister` | 合同侧账号注册 | 企业信息 |
| POST | `/contract/base/getPersonVerifyUrl` | 个人认证 URL | 个人信息 |
| POST | `/contract/base/getCompanyVerifyUrl` | 企业认证 URL | 企业信息 |
| POST | `/designer/settled/getStatus` | 设计师入驻状态 | — |
| POST | `/designer/settled/getRecordDetail` | 入驻记录详情 | — |
| POST | `/designer/settled/addOrUpdate` | 提交/更新入驻 | 资料 |
| POST | `/designer/settled/completeCert` | 完成认证 | 资料 |
| POST | `/designer/settled/getSignContractInfo` | 签约信息 | — |
| POST | `/designer/settled/queryContract` | 查询合同 | — |
| POST | `/designer/settled/withdraw` | **提现** | 金额（写，未执行） |
| POST | `/designer/homepage/knowDesignerByToken` | 按 token 取设计师主页 | `token` |
| POST | `/tu-tag/custom/t-tags/GetByClassifyPage` | 自定义标签分页 | 分类 + 分页 |
| POST | `/tu-tag/custom/t-tags/AddTag` | 新增标签 | `name`、`classify` |
| POST | `/tu-tag/custom/t-tags/GetByTagIds` | 按 id 取标签 | `tagIds[]` |
| POST | `/api/HandleTags/GetTagListByClassfy` | 标签列表（设计侧） | `classify` |
| POST | `/api/HandleTags/CreateUpdateTag` | 新增/改标签 | `name` |
| POST | `/account-search/basic/city/listSimplify` | 城市简表 | — |
| POST | `/account-search/basic/city/CHList` | 城市列表 | — |
| POST | `/basic/city/CHList` | 城市列表（SCM 侧） | — |
| POST | `/account-search/basic/dict/getByKeys` | 数据字典（账号域） | `keys[]` |
| GET | `/account-search/basic/dict/getByKey` | 单个字典（走 `gateway-internal`） | `key` |
| POST | `/platform/basic/dict/getByKeys` | 数据字典（平台域） | `keys[]` |
| POST | `/account-search/web/event/getMyEventCode` | 取事件码 | — |
| POST | `/customer/CustomerInfo/MonthActiveUser` | 月活用户数 | 月份 |
| POST | `/exhibition/report/queryList` | 展会报告列表 | 分页 |
| POST | `/exhibition/report/detail` | 展会报告详情 | `id` |
| POST | `/exhibition/report/getAreaList` | 展会地区列表 | — |
| POST | `/theme-activity/api/Trend/GetTrendList` | 趋势列表 | 分页 + `market`/`trenderType` |
| POST | `/theme-activity/api/Trend/GetTrendDetail` | 趋势详情 | `id` |
| POST | `/theme-activity/api/Trend/GetReferenceIywTuList` | 趋势关联图案 | 趋势 id |
| POST | `/ai-chat/api/trender/GetThemeList` | 趋势主题 | 分页 |
| POST | `/ai-chat/api/trender/GetTrenderList` | 趋势列表（无前导斜杠，源码如此） | 分页 |
| POST | `/ai-chat/api/trender/OptimizePrompt` | 提示词优化 | `prompt` |
| POST | `/ai-chat/api/prompt/refine` / `refine/v2` | 提示词润色（旧/新） | `prompt` |
| POST | `/ai-agent-new/api/refine-prompt` | 提示词润色（新版 Agent） | `prompt` |
| POST | `/ai-chat/api/customerTrender/list` 等 5 个 | 客户趋势：`trenderList`、`add`、`deleteTrender`、`setStatus`、`getProcess` | 趋势资料 |
| POST | `/ai-chat/api/customerRequirement/*` 7 个 | 客户需求：`add`、`getList`、`detail`、`setClose`、`deleteRequirement`、`adoptImage` | 需求资料 |
| POST | `/ai-chat/api/customerRequirementPublish/*` 6 个 | 已发布需求：`getList`、`detail`、`editTitle`、`deleteImages`、`delete`、`generatePDF` | 需求资料 |
| POST | `/ai-chat/api/designScheme/*` 6 个 | 设计方案：`generateImage`、`list`、`detail`、`searchGenerateResult`、`deleteImage`、`deleteScheme`、`generatePdf` | 方案资料 |
| POST | `/ai-chat/api/dialogue/list` / `delete` | 对话列表 / 删除 | 分页 / `id` |
| POST | `/ai-chat/api/favourite/list` / `set` | 收藏列表 / 设置 | `id` |
| POST | `/ai-chat/api/favourite/getPromptList` / `addPrompt` / `deletePrompt` | 提示词收藏 3 个 | `prompt` |
| POST | `/ai-chat/api/imageSearch/search` | 以图搜图 | `imageUrl`、分页 |
| POST | `/ai-chat/api/chat/describe` | 图片描述 | `imageUrl` |
| POST | `/ai-application/api/promptCase/getAllCase` | 提示词案例列表 ✅ 实测样本 146 KB | 分页 |
| POST | `/ai-application/api/geoExperience/apply` | GEO 体验申请 | 申请资料 |
| POST | `/ai-application/faddish/generate` | 语言生成 | 文本 |
| POST | `/iyw-ai-net/api/GenerateTask/GetList` | AI 生成任务列表 | 分页 |
| POST | `/iyw-ai-net/api/HelpVideo/GetVideoList` | 帮助视频 | 分页 |
| POST | `/iyw-ai-net/api/ScaleConfig/GetList` | 缩放配置 | — |
| POST | `/opensearch-service/apply/getMyApply` | 我的 OpenSearch 申请 | — |
| POST | `/tu-zp/api/*` 共 20 个 | 门户作品/概念稿/IP/授权（详见 §1 L0 与历史文档） | 分页 + 筛选 |
| POST | `/tu-userCenter/api/UserBehavior/GetFondClassifications` / `SetFondClassification` | 偏好类目读取 / 设置 | `classifyIds[]` |
| POST | `/ai-agent/api/frogPrince/*` 18 个 | 蛙王子（瓶盖/瓶身设计器）全套：`getBottleList`、`getBottleDetail`、`addBottle`、`updateBottle`、`deleteBottle`、`setBottleShelf`、`getBottleMaterial`、`getBottleNeck`、`searchBottleNeck`，瓶盖侧同名 9 个 | 瓶身/瓶盖资料 |
| GET | `/ai-agent/api/temu/fetchRegionIds` | Temu 地区 id | — |
| GET | `/ai-agent/api/temu/fetchGoodsDetail?goodsId=` | Temu 商品详情 | `goodsId` |
