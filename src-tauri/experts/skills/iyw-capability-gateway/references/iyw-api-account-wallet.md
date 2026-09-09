# 会员、点数、钱包、组织与账号

新增成员/部门/角色详情和方法修正见 [组织业务补充](iyw-api-org-operations.md)，权益与权限见 [L0-L3](iyw-api-access-levels.md)。下方旧方法与字段不自动覆盖新版具体表；merchant/getCurrentOrg 的新资料本身仍有方法冲突。

资料来源：用户提供的 2026-09-09 接口详细版（线上前端包 + 部分登录态实测）。表中的“实测”是原资料的证据标记，不表示本次已调用。未明确的必填字段、类型、枚举、完整表单仍须结合实际页面请求确认。

调用入口：`fetch_iyw_url`。先读 [HTTP 约定](iyw-http.md)，每次提供 `description`；检查 HTTP 信封及业务 `body.code`。这里只提供接口资料，不创建 capability_id，不走 search/read/invoke。检索其他业务见 [接口索引](iyw-api-index.md)。

路径规则：完整 URL 为 `https://gateway.iyw.cn/` + 表中服务路径；开头 `/` 只表示路径根，不改变域名。保留大小写和接口原始拼写。简称接口沿用该行左侧路径的父目录。GET 的参数放 `query`，POST 的参数放 `body`。

检索词：会员权益、余额、点数、算力、消耗明细、充值、支付二维码、组织、部门、员工、角色、数据字典、短信、消息、STS、登录。

查询顺序：当前机构 -> 余额/权益 -> 必要的分页明细。`recordPage` 使用 `pageNum`，不要改成 `page`；点数示例金额和条数是历史数据。表中 `op`、员工业务 `token` 语义未证实，不猜测也不读取主机凭证填入。

登录换 token、刷新/退出、桌面授权、STS、OSS policy 等凭证生命周期端点仅保留检索信息，交给主机账号/上传功能处理，不能用 fetch 把凭证返回给代理。`upload_iyw_file` 已包装上传。`sso/...renewToken` 虽为 GET，仍有状态变化，不属于只读查询。

```json
{"description":"查询可用设计点数","url":"https://gateway.iyw.cn/user-service/employeePoints/getAiPoints","body":{}}
```

```json
{"description":"查询点数消耗记录","url":"https://gateway.iyw.cn/user-service/employeePoints/recordPage","body":{"pageNum":1,"pageSize":20}}
```

接口可能返回成员/手机号等个人信息，只输出完成任务所需摘要。消息发送、短信、支付、提现、额度调整和账号切换需对应用户明确的操作意图；不得作为查询失败的修复动作。

## 接口与参数

## 九、会员、钱包与账号（`member` / `iyw-wallet` / `user-service`）

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 |
| --- | --- | --- | --- | --- |
| POST | `member/api/v2/MemberAuth/GetAuthList` | 会员权益勾选状态 | `authCodes[]` | 实测传入 `["T33","T34","A1","A2","T29","I6","I1","I2","I5","I8","I3","I10","I11","S2","I12"]` ✅ |
| POST | `member/api/v2/MemberAuth/GetMemberAuthInfo` | 会员权益详情 | `authCode` | |
| POST | `member/api/v2/MemberAuth/GetExpiryRemain` | 会员到期剩余 | 无（未登录 403） ✅ |
| POST | `member/api/v2/MemberOrder/GetMemberOrderList` | 会员订单列表 | 分页 | |
| POST | `member/api/v2/MemberOrder/GetComputingPowerVipType` | 算力会员类型 | `keys[]` | 字典键 |
| POST | `member/api/v2/Package/GetDosingPackageList` | 点数套餐列表 | `alias` | |
| POST | `member/api/v2/Message/SendRechargeAIMsg` | 充值提醒消息 | `content`、`price`、`type` | |
| POST | `tu-od/api/Member/GetMemberPackageConfig` | 会员套餐配置 | 无（未登录 403） ✅ |
| POST | `iyw-wallet/api/UserBalance/GetUserBalance` | 钱包余额 | 无（未登录 403） ✅ |
| POST | `iyw-wallet/api/Pay/GetMemberQRCode` | 取支付二维码 | `typeId`、`payMethod`、`device`、`amount` | `payMethod` 支付方式；`device=1` PC；`amount` 数量 ✅ |
| POST | `iyw-wallet/api/Pay/PayPCOrderQuery` | 查订单支付状态 | `payOrderNo` | 轮询用 ✅ |
| POST | `user-service/user/getMyInfo` | 用户信息+菜单+机构 | `orgCode` | `orgCode:0` 当前机构 ✅ 实测 |
| POST | `user-service/user/getInfo` | 用户基础资料 | 无 | |
| POST | `user-service/user/getCurrentUserInfo` | 当前用户 | 无 | |
| POST | `user-service/user/isOldUser` | 是否老用户 | 无 | |
| POST | `user-service/user/createImageAuthCode` | 生成图像授权码 | 无 | |
| POST | `user-service/user/checkoutImageAuthCode` | 校验授权码 | `authCode` | |
| POST | `user-service/user/createPersonSpace` | 创建个人空间 | 表单 | |
| POST | `user-service/user/createCompany` | 创建企业空间 | 表单 | |
| POST | `user-service/org/getOrgMarket` | 机构市场属性 | 无（未登录 403） ✅ |
| POST | `user-service/org/updateMarket` | 改市场属性 | `market` | 0/1/2 |
| POST | `user-service/org/getOrgAvatar` | 机构头像 | 无 | |
| POST | `user-service/org/focus/save` | 保存机构关注 | `orgCode` | |
| POST | `user-service/menu/menuList` | 权限菜单树 | 无（未登录 403） ✅ |
| POST | `user-service/merchant/getCurrentOrg` | 当前机构 | 无（未登录 403） ✅ |
| POST | `user-service/merchant/updateName` / `updateLogo` | 改名称 / Logo | `name` / `logo` | |
| POST | `user-service/dept/list` / `listTwo` / `addOrUpdate` / `transfer` / `updateStatus` | 部门管理 | `deptId`、`name`、`parentId`、`status` | |
| POST | `user-service/employee/list` / `addOrUpdate` / `updateStatus` / `employeeDeptList` / `makeOver` / `generateLink` | 员工管理 | 员工字段 / `employeeId` / `op`、`token` | |
| POST | `user-service/role/list` / `addOrUpdate` / `transfer` / `updateStatus` | 角色管理 | `roleId`、`name`、`status` | |
| POST | `user-service/sms/send` | 短信验证码 | `Mobile`、`Equipment`、`registerSource` | 实测参数含 `Equipment:"pc"`、`platForm:"AI生成平台"` |
| POST | `user-service/aliyun/oss/policy` | OSS 直传凭证 | 无 | 返回 `host` |
| POST | `user-service/employeePoints/getAiPoints` | 可用点数 | 无 | `{availablePoints:141850}` ✅ |
| POST | `user-service/employeePoints/getAiPointsType` | 点数类型枚举 | 无 | ✅ |
| POST | `user-service/employeePoints/typeList` | 消耗类型清单 | 无 | ✅ |
| POST | `user-service/employeePoints/recordPage` | 点数消耗明细 | `pageSize`、`pageNum`、`channel`、`memberName`、`startDate`、`endDate` | 实测 882128 条，字段含手机号/成员名/类型/渠道/时间/消耗点数 ✅ |
| POST | `user-service/employeePoints/page` | 成员点数汇总 | 分页 | |
| POST | `user-service/employeePoints/platUsedPoints` | 平台已用点数 | `alias` | |
| POST | `user-service/employeePoints/updateTotalPoints` | 调整总额度 | `points` | |
| POST | `account-admin/login/product` | 业务侧登录换 token | 登录态 | 登录后换取业务 token |
| POST | `account-admin/revise/switchWorkspace` | 切换空间 | `orgCode` | 多主体切换 ✅ |
| POST | `account-admin/revise/phoneCaptcha` | 手机验证码 | `phone` | |
| POST | `account-search/basic/dict/getByKey` / `getByKeys` | 数据字典（账号域） | `key` / `keys[]` | |
| POST | `account-search/basic/city/listSimplify` / `CHList` | 城市列表 | 无 / `parentId` | |
| POST | `account-search/web/event/getMyEventCode` | 取事件码 | 无 | |
| POST | `platform/basic/dict/getByKeys` | 数据字典（平台域） | `nameSpace`、`keys[]` | 实测 `nameSpace:"COMMON"` + 9 个 key ✅ |
| POST | `platform/oss/securityToken` / `securityNewToken` | OSS STS 令牌 | 无 | |
| GET | `sso/api/sso/web/renewToken?refreshToken={t}` | 刷新登录态 | `refreshToken`（querystring） | ✅ 实测 |
| POST | `sso/api/sso/web/discardToken` | 退出登录 | 无 | |
| POST | `messagecenter/Message/RecvMsgParser` | 消息解析 | `msgHandleCode`、`content` | 工单消息 |
| POST | `messagecenter/Message/SendWeComGroupMsg` | 企微群消息 | `content` | |
| POST | `user-service/menu/menuList` | 菜单（AI 侧） | 无 | |
| POST | `user-service/dept/listTwo` | 二级部门 | `parentId` | |
