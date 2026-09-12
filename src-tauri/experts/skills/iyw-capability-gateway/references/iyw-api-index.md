# 爱原物业务接口索引

爱原物设计云、AI 工作台、图案网业务：先从下表选择领域，只读取相关文件和段落，再用 `fetch_iyw_url` 执行。不要一次加载全部接口；不要为业务接口搜索或编造 capability_id，也不要创建额外 MCP。

图片生成/处理使用 `generate_iyw_image`；任意文件上传使用 `upload_iyw_file`；剩余业务接口统一使用 `fetch_iyw_url`。已有 `search_iyw_knowledge` 正文检索、主机记忆/浏览器等能力维持各自现有路由。

## 按任务检索

| 关键词 / 用户意图 | 读取资料 | 服务 / 典型接口 |
| --- | --- | --- |
| 原助理、本体、智能体会话、历史、继续、停止、重命名、收藏 | [会话](iyw-api-conversations.md) | `ai-agent`、`conversation/list`、`chat_continue` |
| 知识库目录、文件、解析状态、自动报告、报告分享、提示词精炼 | [知识与报告](iyw-api-knowledge-reports.md) | `ai-agent-new`、`knowledge/folders/list`、`trend-push` |
| 产品库、产品搜索、基础款、公开、产品标签、批量标签 | [产品](iyw-api-products.md) | `userProduct`、`userProductTags`、`userProductTagsIndex` |
| 图片生成记录、任务进度、回收站、素材、提示词案例、PDF 购物车/导出 | [图片业务管理](iyw-api-image-admin.md) | `commerce/getCommerceTaskDetail`、`microModel/GetDetails` |
| 客户需求、需求发布单、客户趋势、设计稿、图片搜索、采购目录、提示词收藏 | [客户与设计](iyw-api-customer-design.md) | `ai-chat`、`customerRequirement`、`designScheme` |
| 正版图案、热门作品、IP、概念稿、民艺、展会、类目授权、系列、订单 | [图案与授权](iyw-api-patterns.md) | `tu-zp`、`Creation`、`Ip`、`Empower` |
| 趋势主题、报告详情、关联图案、内销/外销、趋势 PDF | [趋势](iyw-api-trends.md) | `theme-activity`、`Trend/GetTrendDetail` |
| 瓶型、瓶盖、材质、瓶颈、上下架、Temu 商品、区域 | [包装与电商](iyw-api-packaging-commerce.md) | `frogPrince`、`temu` |
| 会员、权益、点数、余额、支付、组织、员工、部门、角色、字典、短信、消息 | [账号与钱包](iyw-api-account-wallet.md) | `member`、`iyw-wallet`、`user-service`、`platform` |
| 需求大厅、比稿、稿件、报名、投稿、设计师、入驻、作品分类 | [门户需求与作品](iyw-api-portal-demand-creation.md) | `/Demand`、`/Creation`、`/designer/settled` |
| 版权登记、证书、模板、提现、充值、合同、签署、实名认证 | [门户版权与财务](iyw-api-portal-copyright-finance.md) | `/Copyright`、`/Finance`、`/Contract` |
| 用户中心、收藏分组、店铺、企业展厅、资质、帮助、投诉、文档解析 | [门户用户与内容](iyw-api-portal-account-content.md) | `/User`、`/Shop`、`/Enterprise`、`/Help` |
| 生图、变款、重绘、扩图、放大、抠图、蒙版、消除、矢量、色号、3D、视频 | [图片工具参数](iyw-image-tools.md)，仅核对原文时读 [图片接口证据](iyw-image-api-source.md) | `generate_iyw_image` |
| 上传文件、文档、压缩包、音频、视频、50M、文件 URL | [通用上传](iyw-upload.md) | `upload_iyw_file` |

## 三层披露

1. Skill 名称/描述负责触发；此索引只负责业务选择。
2. 阅读所选业务参考与 [HTTP 约定](iyw-http.md)，选定精确方法、路径和参数。
3. 仅在当前操作需要时读取该参考中的完整接口行、示例、字段限制；已有知识可复用，不重复全量加载。

如果只有接口片段或关键词，在 skill 目录执行定向文本检索，不联网、不执行接口：

```bash
rg -n -i -F -e '产品' -e 'getProductList' references -g 'iyw-api-*.md'
rg -n -i -F -e '提现' -e 'Withdrawal' references -g 'iyw-api-*.md'
```

不支持 shell 的宿主可按上表直接读取相对文件。搜索结果仅帮助定位；调用前仍需读取对应说明。所有参考文件随内置 skill 打包，无需访问用户 Downloads 路径。

## 请求与结果

- `description` 写具体动作，例如“查询基础款产品”或“更新产品标签”；不能写成接口名，不能包含凭证或个人信息。界面显示该动作，主机不向业务 API 发送它。
- API host 默认 `https://gateway.iyw.cn`。门户业务使用 `https://www.iyw.cn/gateway`；`/msgapi` 只依据实际页面已确认请求选用，不能在写操作失败后换代理重放。
- 本资料中的业务方法使用 GET/POST。GET 参数放 `query`；POST 参数放 `body`。方法、大小写、拼写、数组与分页字段逐接口保留。
- `code=1` 是原资料的常见业务成功码；`0` 参数错误、`2` 业务拒绝、`403` 未登录。先检查工具 `ok/status`，再检查 `body` 的业务状态。HTTP 200 不是业务成功。
- 原始列表通常是 `data.list` + `totalCount/total`，也可能为数组；根据实际返回解释，不能把当前页数量当总数。ID 从用户或前一步真实记录取得。
- 先用小分页并保留筛选；需要全部时逐页读取，直到总数已满足或返回空页，避免一次拉取超过 2 MiB。不要把 `page/page_size`、`page/pageSize`、`pageNum/pageSize`、`pageIndex/pageSize` 混用。
- 上下架、收藏、发布、删除、支付、发消息等均是业务动作；只执行用户当前任务授权的内容，未知结果不自动重放。

## 来源与不确定性

来源：2026-09-09《爱原物接口文档·详细版（含请求类型与入参说明）》，从两站前端包整理并对部分查询做过登录态实测。这里保留所有接口表行，拆成领域参考；原资料“实测”不表示本次已请求。

原文汇总“192 个接口、16 个 GET”与正文多接口合并行不一致；以正文实际接口行和完整路径为准，不以汇总数宣称已实测。只写“分页”“表单”“同上”的地方不构成完整 schema；必填性、枚举和返回格式未知时应核对原页面实际请求。可继续完成独立查询，但不虚构缺失字段。

登录、刷新、退出、STS、OSS 签名等凭证步骤由主机账号/上传功能处理。资料保留这些端点以便检索和解释归属；代理不通过 fetch 读取或传递登录凭证。

流式聊天、超过 2 MiB 的响应、multipart 文件接口及未确定服务域的 `/api/*` 路径可能超出 fetch 的当前传输能力。必须说明具体限制，不能报告已执行；原资料不能证明这些接口已完全可用。
