# 补充调用约定、证据与契约冲突

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

这些是源站约定；当前 MCP 使用主机登录、禁止传 token/tokenInfo，默认 60 秒，可用 timeout_seconds 设置 1-900 秒，不继承站点自动重试。L0 表示上游允许公开访问，fetch 当前仍需要主机登录。SSE 仍聚合为文本，响应仍限制 2 MiB，WS 不在 fetch 支持范围。

签名摘要缺少盐值、完整触发接口清单和可验证样例。不得猜盐、硬编码秘密或因失败重复签名写操作；凭证和 STS 交给主机上传/账号功能，未支持的签名业务明确说明限制。`securityKey:test` 只在部分来源摘要出现，不能推导为所有接口的强制认证头。

## 冲突优先级

具体调用点/完整字段表优先于同文的概括表；已确认运行契约优先于新版未实测摘要。不要仅因日期较新就替换参数，冲突时核对实际页面请求。

| 冲突 | 执行规则 |
| --- | --- |
| knowledge/folders/list 的 folderId 分页旧示例与补充 §5.1 | 新版管理用 category + parentId 字符串；原旧示例只作历史证据 |
| trend-push 分享 reset 自定义 sharePassword 与补充 §7 | 新版只传 run_id，由服务返回新口令 |
| menu/menuList、getAiPointsType | 补充具体表为 GET；旧 POST 标记为历史，不同时调用两种方法 |
| merchant/getCurrentOrg | 补充 L1 为 GET、§9.8 为 POST，补充内部仍冲突；使用当前已观察请求，不能假装已唯一确认 |
| PreSignedUrl 三版 objectKey / fileName+contentType / fileName+fileType | 现有已确认 uploader 保留 objectKey；补充没有完整可验证替代协议 |
| microModel/v2/batch 的 models/jsonData / items / prompts | 保留已确认分身 models/jsonData；不按汇总表重写 |
| checkImage、GetImageSegment、microModel/upscale/variation | 多版 image/imageUrl/imageUrls/taskId 不一致；已实现类型继续使用对应明确契约，其他模式保留检索证据 |
| commerce g_tools/f_tools、classifyCanvasIntent、watermarkEraser、bleedLine | 已支持补充明确的 toolName+imageUrls、text、target、bleed 变体；不要混用无关操作字段 |
| 用户产品、短信、账号切换、订单支付查询的单复数与 ID 字段 | 原文与补充都是部分摘要，按当前实际契约核对；不靠错误重试猜字段 |
| 概括分页表 | 具体端点表优先；knowledge 文件仍用 page/pageSize，trend-push 用 page/page_size |

`tuapi.chdesign.cn` 不在 fetch 的 iyw.cn 域名边界；WS 地址、SCM 演示地址不能作为生产 HTTP 端点。原资料所称 `iyw-api/samples/` 是采集环境路径，本 skill 不假定那些样本已存在。

## 来源详细资料

## 零、先看这一章：公共调用约定（升级版）

### 0.1 网关与代理地址

| 地址 | 用途 | 说明 |
| --- | --- | --- |
| `https://gateway.iyw.cn` | **主网关**（两站共用） | 绝大多数接口的真正地址 |
| `https://www.iyw.cn/gateway` | 门户代理 | 门户页面里看到的是相对路径，实际转发到主网关 |
| `https://www.iyw.cn/msgapi` | 消息代理 | 消息中心相关 |
| `https://gateway-internal.iyw.cn` | 内部网关 | `VUE_APP_DESIGN_BASE`，设计侧内部调用 |
| `https://tuapi.chdesign.cn` | 图案设计 API | `VUE_APP_DESIGN_API` |
| `wss://ai.iyw.cn/ai-app/api/message/notification/ws` | 消息推送 WebSocket | 站内通知 |
| `wss://ai.iyw.cn/agent/api/generate_pdf/ws/generate-pdf` | PDF 生成进度 WebSocket | PDF 导出进度推送 |

### 0.2 认证方式（三种并存）

| 方式 | 取值来源 | 出现在哪些请求 |
| --- | --- | --- |
| 请求头 `token` | Cookie `iyuanwu_token` | 绝大多数 AI 站接口（拦截器自动注入） |
| 请求头 `tokenInfo` | Cookie `tokenInfo` / `agent_chat_token_info` | **仅新版 Agent**（`ai-agent-new`）在有开发 Token 时附带 |
| Cookie 直带 | 浏览器自动携带 `iyuanwu_token` | 门户侧（`www.iyw.cn`）接口 |

**新版 Agent 的特殊规则**（源码可证）：

- 普通模式：`base = https://gateway.iyw.cn`，前缀 `/ai-agent-new/api/agent` 或 `/ai-agent-new/api/knowledge`
- 开发模式（Cookie 里存在 `agent_new_dev_token`）：走 `VUE_APP_AGENT_NEW_API_ORIGIN`，前缀变成 `/api/agent` 或 `/api/knowledge`，并且手动带上 `token` + `tokenInfo` 两个头

### 0.3 签名与公共字段（重要，之前文档没写）

部分接口（`sign:true`）会在 body 里**自动追加**三个字段：

| 字段 | 值 | 含义 |
| --- | --- | --- |
| `ts` | `Date.parse(new Date()).toString()` | 毫秒时间戳字符串 |
| `sysName` | `"Front_CUMS"` | 调用方系统名 |
| `version` | `"0.1.0"` | 前端版本号 |
| `sign` | `md5(("Front_CUMS" + "0.1.0" + ts + JSON.stringify(排序后的业务参数) + 盐).toUpperCase())` | 签名，业务参数按 key 名**倒序**排序后再序列化 |

注意：如果接口带 `data` 字段，整个业务参数会被**包一层** `data`：`{ts, sign, sysName, version, data: 业务参数}`。

### 0.4 统一响应结构

```json
{ "code": 1, "message": "success", "data": {} }
```

| code | 含义 | 前端行为 |
| --- | --- | --- |
| 1 | 成功 | 正常取 `data` |
| 0 | 参数错误 | 弹错误提示 |
| 2 | 业务校验失败 | 弹 `message`（除非 `skipErrorToast`） |
| 403 / 404 | 未登录 / token 失效 | 清登录态、弹「请先登录！」、打开登录弹窗 |

**重试策略**（axios 拦截器）：`timeout=300000`（300 秒）、`retry=3`、`retryDelay=1000`，仅在 `ECONNABORTED`、HTTP 504、`Network Error` 时重试。

### 0.5 分页与筛选命名规律

| 风格 | 出现位置 | 字段 |
| --- | --- | --- |
| 下划线（新版） | `ai-agent`、`ai-agent-new`、`microModel`、`commerce` | `page` + `page_size` |
| 驼峰（旧版） | `tu-zp`、`ai-chat`、`ai-application` | `pageNum` / `page` + `pageSize` |
| 中括号法 | `pageIndex` + `pageSize` | `theme-activity`、`ai-chat/api/trender` |

筛选三类布尔统一用数字：`-1` 全部 / `0` 否 / `1` 是（如 `isBasic`、`public`）。


## 十、统计、证据等级与接入建议

### 10.1 总量统计

| 项 | 数量 | 说明 |
| --- | --- | --- |
| 两站源码提取的唯一路径 | **371 条** | 从 `ai_app`、`chunks/*`、`i_app`、`nuxt_*` 全量正则提取 `url:"..."` |
| 已整理成表（含本篇） | **约 300 条** | 历史文档 192 条 + 本篇新增 §2~§9 |
| 本篇新增接口 | **约 110 条** | 新版 Agent 11、知识库 23、商品套图 11、批量中心 10、趋势推送 8、门户补齐约 150 项 |
| GET 接口 | 约 30 条 | 主要为 `sessions`、`messages`、`runs/current`、`menuList`、`get/{id}` 系列、`renewToken`、`dict/getByKey`、Temu 详情 |
| POST 接口 | 约 270 条 | 其余全部 |
| DELETE 接口 | 4 条 | `favorites/{id}`、`sessions/{id}`、`messages/{id}`（新版 Agent） |
| WebSocket | 2 条 | 消息通知、PDF 进度 |

### 10.2 服务组分布

| 服务组 | 前缀 | 大致数量 | 主要用途 |
| --- | --- | --- | --- |
| 新版 Agent | `/ai-agent-new/api/agent` | 11 | 会话、消息、生成 |
| 知识库 | `/ai-agent-new/api/knowledge` | 23 | 文件夹、文件、切片、检索 |
| 商品套图 | `/ai-agent-new/api/product-kit` | 11 | 电商套图、A+ 详情图、Listing |
| 趋势推送 | `/ai-agent-new/api/trend-push` | 8 | 智配报告、分享 |
| 生图任务 | `/ai-application/api/microModel` | 23 | 文生图、改图、收藏 |
| 电商工具 | `/ai-application/api/commerce` | 38 | 换背景、扩图、抠图、3D 等 |
| 批量中心 | `/ai-application/api/batch*` | 10+ | 批量工具 |
| 用户产品库 | `/ai-application/api/userProduct*` | 15 | 商品库与标签 |
| AI 对话 | `/ai-chat/api/*` | 约 40 | 需求、趋势、方案、收藏 |
| 门户设计 | `/tu-zp/api/*`、`/tu-userCenter/*`、`/tu-tag/*` | 约 26 | 作品、概念稿、IP |
| 用户与企业 | `/user-service/*` | 约 45 | 用户、组织、员工、部门、角色、点数 |
| 会员与支付 | `/member/*`、`/User/*`、`/Home/*`、`/iyw-wallet/*` | 约 25 | 权益、订单、支付 |
| 资产库 | `/saas-assets/*` | 约 45 | 作品与证书管理 |
| 工厂/物流/图案订单 | `/factoryOrder/*` 等 | 22 | 供应链 |
| 其他 | 消息、字典、展会、帮助等 | 约 40 | 零散 |

### 10.3 证据等级

| 等级 | 含义 | 覆盖 |
| --- | --- | --- |
| ✅ 实测 | 用真实账号调用过并保存了响应样本 | `getMyInfo`、`employeePoints` 系列、`recordPage`、`typeList`、`conceptDraft`、`trend` 部分、`promptCase` 等十余个 |
| ⚙️ 源码提取 | 从前端打包文件反解出的调用签名（url + method + data 结构），**未实际调用** | 其余绝大部分 |
| 📄 仅出现 | 只在配置文件/常量表里出现，未见调用点 | 少数环境变量默认路径 |

响应样本存在 `iyw-api/samples/` 目录，例如 `auth_user-service_user_getMyInfo.json`（12.6 KB，含菜单树与机构列表）、`auth_user-service_employeePoints_typeList.json`（5 KB，点数类型清单）、`auth_user-service_employeePoints_recordPage.json`（2.2 KB，消耗流水）、`auth_ai-application_api_promptCase_getAllCase.json`（146 KB，提示词案例）。

### 10.4 接入建议

1. **不要直连前端 Cookie**：认证字段是 `token` 请求头（Cookie 名 `iyuanwu_token`），有效期内可复用；过期用 `sso/api/sso/web/renewToken?refreshToken=` 刷新。
2. **签名接口优先**：`sign:true` 的接口（`platform/oss/securityToken` 等）必须按 §0.3 的算法拼签名，否则返回参数错误。
3. **写操作先确认**：本文件中的所有「下单、支付、发货、删除、提现、调整额度、创建空间」接口都只做了源码确认，**没有实际执行**；接入时务必用测试账号先验证。
4. **分页字段别混用**：`ai-agent-new` 用 `page` + `page_size`；`tu-zp`、`ai-chat` 用 `pageNum` + `pageSize`；`theme-activity` 用 `pageIndex` + `pageSize`。
5. **SSE 要处理重连**：`/chat`、`/attachments/refresh-stream` 是流式的，中途断开会丢事件；前端是用 `AbortController` + `/runs/current` 恢复，接入方应实现同样机制。
6. **长耗时接口单独设超时**：批量 `packDownload`、`folders/delete-tree`、`files/create`、`generate-kit`、`generate-a-plus` 的超时是 300~650 秒，别用默认 30 秒。

### 10.5 已知限制

- 本文档是从**前端打包代码**反解，并非官方接口文档；服务端可能对某些字段做了额外校验或存在未在前端暴露的参数。
- 部分接口名存在拼写问题（`task/detele`、`PackageUseDeatail`、`ai-chat/api/trender/GetTrenderList` 缺前导斜杠），本文按源码原文保留，接入时以实际为准。
- `/finance/*`、`/payLog/export` 等财务接口在门户侧存在但样本不足，未展开；`scm-demo.iyw.cn/api` 供应链域名属于演示环境，生产环境以 `VUE_APP_PRODUCT_URL` 配置为准。
- 未收录：静态资源、埋点上报、第三方 SDK（TOS/OSS/Photopea）内部请求。

---

> 本文档由爱原物原助理整理，覆盖 `https://www.iyw.cn` 与 `https://ai.iyw.cn` 两个站的前端可见接口。
