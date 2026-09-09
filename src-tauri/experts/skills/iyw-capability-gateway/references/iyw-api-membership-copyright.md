# 会员订单、支付与版权补充

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

完整 URL 使用 https://gateway.iyw.cn + 表中路径；按本节来源方法调用 fetch_iyw_url。`/Certify/CheckCertifys` 的行内 GET 优先于章节“全部 POST”。旧门户代理路径也是历史来源，不在写失败后切换地址重放。

订单 ID、payOrderNo/orderId、下载证书 ids/recordIds、关闭登记 recordId/id 多版摘要存在差异，核对实际请求，不能靠重复支付/删除尝试参数。权益字段见 [权限分层](iyw-api-access-levels.md)。

## 来源详细资料

### 9.4 会员 / 订单 / 支付（全部 POST）

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/member/api/v2/MemberAuth/GetAuthList` | 权益列表（AI 站启动必调） | `authCodes[]`（详见 §1 L2） |
| `/member/api/v2/MemberAuth/GetMemberAuthInfo` | 单项权益详情 | `authCode` |
| `/member/api/v2/MemberOrder/GetMemberOrderList` | 会员订单列表 | 分页 |
| `/member/api/v2/MemberOrder/GetComputingPowerVipType` | 算力会员类型 | — |
| `/member/api/v2/Package/GetDosingPackageList` | 套餐包列表 | — |
| `/member/api/v2/Message/SendRechargeAIMsg` | 发送充值 AI 消息 | 充值信息 |
| `/member/User/GetSaleId` | 取销售 ID | — |
| `/User/GetMemberAuthPackages` | 会员权益套餐包 | — |
| `/User/GetUserInfo` | 用户信息（版权站视角） | — |
| `/Home/SaveSaaSMemberOrder` | **保存 SaaS 会员订单** | 套餐信息；写操作，未执行 |
| `/Home/GetOrderDetail` | 订单详情 | `orderId` |
| `/Home/GetPayCode` | **取支付二维码** | `orderId`；写操作，未执行 |
| `/Home/CloseOrder` | 关闭订单 | `orderId` |
| `/Home/CheckOrderResult` | 轮询支付结果 | `orderId` |
| `/Member/OrderDetail` | 会员订单详情 | `orderId` |
| `/Member/PackageUseDeatail` | 套餐使用明细（接口名拼写如此） | 分页 |
| `/iyw-wallet/api/Pay/GetMemberQRCode` | 会员支付二维码 | 订单信息 |
| `/iyw-wallet/api/Pay/PayPCOrderQuery` | PC 支付结果查询 | `orderId` |

### 9.5 版权登记（全部 POST）

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/Copyright/CopyrightRecordList` | 版权登记列表 | 分页 |
| `/Copyright/GetCopyrightRecordDetail` | 登记详情 | `recordId` |
| `/Copyright/DeleteCopyrightById` | 删除登记 | `id` |
| `/Copyright/DownloadCertificates` | 下载证书 | `ids[]` |
| `/Certify/CheckCertifys`（GET） | 校验认证状态 | `data` |
| `/UserCopyrightRegister/GetCopyrightBasic` | 登记基础信息 | — |
| `/UserCopyrightRegister/GetTemplateList` | 模板列表 | 分页 |
| `/UserCopyrightRegister/GetTemplateDetail` | 模板详情 | `templateId` |
| `/UserCopyrightRegister/DeleteTemplate` | 删除模板 | `templateId` |
| `/UserCopyrightRegister/GetCertifyInfoList` | 认证信息列表 | — |
| `/UserCopyrightRegister/CheckCopyrightAuth` | 校验登记权限 | — |
| `/UserCopyrightRegister/CheckCopyrightTitle` | 校验作品名称 | `title` |
| `/UserCopyrightRegister/ShutDownCopyrightRecord` | 关闭登记记录 | `recordId` |
| `/copyright/Home/OnlineConsultation` | 版权在线咨询 | 咨询内容 |
