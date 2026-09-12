# 版权登记、财务、合同与实名认证

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：本域所有表格路径加在 `https://www.iyw.cn/gateway` 后。主机自动注入当前登录态对应的 `iyuanwu_token` Cookie；不要提供 Cookie/token 参数。认证失败时恢复主机登录，不从浏览器提取凭证。

检索词：版权登记、证书、认证、模板、字数、提现、充值、支付结果、合同、签署、Copyright、Finance、Contract。

流程：记录列表 -> 真实记录 ID -> 详情/证书。下载证书可能返回链接或二进制；fetch 原始响应上限 2 MiB，大文件使用已返回的下载 URL，不能盲目重复请求。`recordId`、`recordIds`、`id` 保留接口实际字段差异。模板删除、关闭登记、提现、签署状态变更只在相应任务已授权时进行。

```json
{"description":"查询版权登记基础信息","url":"https://www.iyw.cn/gateway/UserCopyrightRegister/GetCopyrightBasic","body":{}}
```

## 接口与参数

### 10.3 版权与财务

| 请求类型 | 接口 | 功能 | 入参 |
| --- | --- | --- | --- |
| POST | `/Copyright/CopyrightRecordList` | 版权记录列表 | 分页、`status` |
| POST | `/Copyright/GetCopyrightRecordDetail` | 版权记录详情 | `recordId` |
| POST | `/Copyright/DownloadCertificates` | 下载证书 | `recordIds[]` |
| POST | `/Copyright/DeleteCopyrightById` | 删除记录 | `id` |
| POST | `/UserCopyrightRegister/GetCopyrightBasic` | 登记基础信息 | 无 |
| POST | `/UserCopyrightRegister/GetCertifyInfoList` | 认证信息列表 | 分页 |
| POST | `/UserCopyrightRegister/CheckCopyrightAuth` | 登记权限校验 | 无 |
| POST | `/UserCopyrightRegister/CheckCopyrightTitle` | 标题查重 | `title` |
| POST | `/UserCopyrightRegister/ShutDownCopyrightRecord` | 关闭登记 | `id` |
| POST | `/UserCopyrightRegister/GetTemplateList` / `GetTemplateDetail` / `DeleteTemplate` | 模板管理 | 分页 / `templateId` / `id` |
| POST | `/UserCopyrightRegister/ComputeFileWordCountBatch` | 批量算字数 | `fileIds[]` |
| POST | `/Finance/GetUserBalanceInfo` | 余额信息 | 无 |
| POST | `/Finance/ApplyWithdrawal` | 申请提现 | `amount`、`bankCardId` |
| POST | `/Finance/GetWithdrawalLog` | 提现记录 | 分页 |
| POST | `/Finance/GetPayResult` | 支付结果 | `orderNo` |
| POST | `/Finance/GetRechargeOrderOptionList` | 充值选项 | 无 |
| POST | `/Finance/OrderReturnQRCode` | 订单支付二维码 | `orderNo` |
| POST | `/Finance/RefreshPaymentQRCode` | 刷新二维码 | `orderNo` |
| POST | `/Contract/GetContractLink` | 合同链接 | `contractId` |
| POST | `/Contract/GetSignedContractLink` | 已签合同链接 | `contractId` |
| POST | `/Contract/UpdateContractSigneStatus` | 更新签署状态 | `contractId`、`status` |
| POST | `/Certification/GetVerifyInfo` / `GetVerifyUrl` / `ChangeFirstSigner` | 实名认证信息 / 跳转 / 变更签署人 | `type` / `type` / 表单 |

